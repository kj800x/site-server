//! Optional integration with an external "shop" service that time-limits which
//! items' media are visible (an incentive layer; honor system, render-time only).
//!
//! Everything shop-related lives in this module. The rest of the code base only
//! calls [`media`] (via the media helpers in `handlers`), [`badge`] (in the shared
//! header), [`tag_unlock`] (on tag listing pages), and [`init`] / [`configure`]
//! (in `main.rs`). To remove the feature: delete this file, make the media helpers
//! return their markup directly, and drop the other calls.
//!
//! Configuration (all optional; unset `SHOP_URL` = feature off, zero behavior change):
//! - `SHOP_URL`: base URL the server uses to reach the shop API.
//! - `SHOP_PUBLIC_URL`: base URL for links rendered into pages (defaults to `SHOP_URL`).
//! - `SHOP_API_TOKEN`: sent as `Authorization: Bearer <token>`.
//!
//! The shop fails open: everything is visible when it is unconfigured, reports
//! `enabled: false`, or has not been fetched successfully for 5 minutes.
//!
//! Signed-in users: locks apply only to signed-in viewers. Every page this server
//! renders sits behind the auth middleware when `BASIC_AUTH_*` is configured, and
//! with no credentials configured the server is single-user with no auth, so the
//! viewer of any rendered page is treated as signed in.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

use actix_web::http::header::{HeaderValue, LOCATION, REFERER};
use actix_web::{web, HttpRequest, HttpResponse};
use maud::{html, Markup};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::site::{CrawlItem, CrawlTag};
use crate::thread_safe_work_dir::ThreadSafeWorkDir;

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const STALE_AFTER: Duration = Duration::from_secs(5 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// Configuration and shared state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ShopConfig {
    pub url: String,
    pub public_url: String,
    pub token: Option<String>,
}

impl ShopConfig {
    pub fn from_env() -> Option<Self> {
        let non_empty = |name: &str| std::env::var(name).ok().filter(|s| !s.trim().is_empty());
        let url = non_empty("SHOP_URL")?.trim_end_matches('/').to_string();
        let public_url = non_empty("SHOP_PUBLIC_URL")
            .map(|s| s.trim_end_matches('/').to_string())
            .unwrap_or_else(|| url.clone());
        Some(ShopConfig {
            url,
            public_url,
            token: non_empty("SHOP_API_TOKEN"),
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entitlements {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub grace_imported_within_ms: u64,
    #[serde(default)]
    pub balance: i64,
    #[serde(default)]
    pub grants: Vec<Grant>,
    #[serde(default)]
    pub tokens: Tokens,
}

/// A grant. Kept as a flat struct so unknown kinds/fields deserialize fine and
/// are simply ignored by the matcher.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default, rename = "ref")]
    pub item_ref: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub seed: Option<String>,
    #[serde(default)]
    pub fraction: Option<f64>,
    #[serde(default)]
    pub min_items: Option<usize>,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub expires_at: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    #[serde(default)]
    pub item: u64,
    #[serde(default)]
    pub tag: u64,
}

/// The latest successful entitlements fetch, plus values derived from the catalog
/// at fetch time (so nothing expensive happens per render).
struct Snapshot {
    ent: Entitlements,
    fetched_at: Instant,
    /// randomTag grant id -> resolved tag key
    random_tags: HashMap<String, String>,
    /// tag display value (what tag pages are keyed by) -> tag keys with that value
    tag_keys_by_value: HashMap<String, Vec<String>>,
}

struct Shop {
    config: ShopConfig,
    client: reqwest::Client,
    workdirs: Vec<ThreadSafeWorkDir>,
    snapshot: RwLock<Option<Snapshot>>,
}

static SHOP: OnceLock<Shop> = OnceLock::new();

/// Read config from env and, if `SHOP_URL` is set, start the background poller.
/// Must be called from within the actix runtime.
pub fn init(workdirs: Vec<ThreadSafeWorkDir>, client: reqwest::Client) {
    let Some(config) = ShopConfig::from_env() else {
        return;
    };
    log::info!("Shop integration enabled ({})", config.url);
    let shop = Shop {
        config,
        client,
        workdirs,
        snapshot: RwLock::new(None),
    };
    if SHOP.set(shop).is_err() {
        return;
    }
    actix_web::rt::spawn(async {
        loop {
            refresh().await;
            actix_web::rt::time::sleep(POLL_INTERVAL).await;
        }
    });
}

/// Register the shop's routes (no-op when the shop is not configured).
pub fn configure(cfg: &mut web::ServiceConfig) {
    if SHOP.get().is_some() {
        cfg.route("/shop/redeem", web::post().to(redeem_handler));
    }
}

/// Run `f` against the current snapshot if the shop is on; `None` means "off"
/// (unconfigured, disabled, never fetched, or stale) and callers fail open.
fn with_active<R>(f: impl FnOnce(&Shop, &Snapshot) -> R) -> Option<R> {
    let shop = SHOP.get()?;
    let guard = shop.snapshot.read().ok()?;
    let snap = guard.as_ref()?;
    if !snap.ent.enabled || snap.fetched_at.elapsed() > STALE_AFTER {
        return None;
    }
    Some(f(shop, snap))
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

// ---------------------------------------------------------------------------
// Polling
// ---------------------------------------------------------------------------

async fn refresh() {
    let Some(shop) = SHOP.get() else {
        return;
    };
    let mut req = shop
        .client
        .get(format!("{}/api/v1/entitlements", shop.config.url))
        .timeout(REQUEST_TIMEOUT);
    if let Some(token) = &shop.config.token {
        req = req.bearer_auth(token);
    }
    let ent = match req.send().await.and_then(|r| r.error_for_status()) {
        Ok(resp) => match resp.json::<Entitlements>().await {
            Ok(ent) => ent,
            Err(e) => {
                log::warn!("Shop entitlements: bad response: {}", e);
                return;
            }
        },
        Err(e) => {
            log::warn!("Shop entitlements fetch failed: {}", e);
            return;
        }
    };

    // Tag counts across all sites, computed off the async thread (workdir locks).
    let workdirs = shop.workdirs.clone();
    let (tag_counts, tag_keys_by_value) = web::block(move || catalog_tags(&workdirs))
        .await
        .unwrap_or_default();

    let random_tags = ent
        .grants
        .iter()
        .filter(|g| g.kind == "randomTag")
        .filter_map(|g| {
            let seed = g.seed.as_deref()?;
            let tag = pick_random_tag(seed, g.min_items.unwrap_or(0), &tag_counts)?;
            Some((g.id.clone(), tag))
        })
        .collect();

    let snap = Snapshot {
        ent,
        fetched_at: Instant::now(),
        random_tags,
        tag_keys_by_value,
    };
    if let Ok(mut guard) = shop.snapshot.write() {
        *guard = Some(snap);
    }
}

type TagCatalog = (HashMap<String, usize>, HashMap<String, Vec<String>>);

/// Tag key -> item count across all sites, and tag display value -> its keys.
fn catalog_tags(workdirs: &[ThreadSafeWorkDir]) -> TagCatalog {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut by_value: HashMap<String, Vec<String>> = HashMap::new();
    for wd in workdirs {
        let Ok(wd) = wd.work_dir.read() else { continue };
        for item in wd.crawled.items.values() {
            for tag in &item.tags {
                let key = tag_key(tag);
                let n = counts.entry(key.clone()).or_insert(0);
                if *n == 0 {
                    by_value.entry(tag.to_string()).or_default().push(key);
                }
                *n += 1;
            }
        }
    }
    for keys in by_value.values_mut() {
        keys.sort();
    }
    (counts, by_value)
}

// ---------------------------------------------------------------------------
// Matching (pure functions)
// ---------------------------------------------------------------------------

fn digest(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
}

/// `u32_be(SHA-256(seed + ":" + ref)[0..4]) / 2^32`, in [0, 1).
pub fn score(seed: &str, item_ref: &str) -> f64 {
    let d = digest(&format!("{}:{}", seed, item_ref));
    u32::from_be_bytes([d[0], d[1], d[2], d[3]]) as f64 / 4294967296.0
}

/// `SHA-256(seed + ":tag:" + tag)`, compared as bytes.
pub fn tag_rank(seed: &str, tag: &str) -> [u8; 32] {
    digest(&format!("{}:tag:{}", seed, tag))
}

/// Among tags with at least `min_items` items (or all tags, if none qualify),
/// the one with the smallest `tag_rank`.
pub fn pick_random_tag(
    seed: &str,
    min_items: usize,
    tag_counts: &HashMap<String, usize>,
) -> Option<String> {
    let qualifying: Vec<&String> = tag_counts
        .iter()
        .filter(|(_, &n)| n >= min_items)
        .map(|(t, _)| t)
        .collect();
    let pool: Vec<&String> = if qualifying.is_empty() {
        tag_counts.keys().collect()
    } else {
        qualifying
    };
    pool.into_iter().min_by_key(|t| tag_rank(seed, t)).cloned()
}

/// `item.meta.shopRef` if present, else `site:{site_slug}:{item.key}`.
/// In the aggregated "all" view keys are namespaced as `{site_slug}/{key}`; that
/// prefix is stripped so refs are the same in every view.
pub fn item_ref(item: &CrawlItem) -> String {
    if let Some(r) = item.meta.get("shopRef").and_then(|v| v.as_str()) {
        return r.to_string();
    }
    let slug = &item.site_settings.site_slug;
    let key = item
        .key
        .strip_prefix(&format!("{}/", slug))
        .unwrap_or(&item.key);
    format!("site:{}:{}", slug, key)
}

/// Simple tag -> `value`; grouped tag -> `{group}:{value}`.
pub fn tag_key(tag: &CrawlTag) -> String {
    match tag {
        CrawlTag::Simple(v) => v.clone(),
        CrawlTag::Detailed { group, value } => format!("{}:{}", group, value),
    }
}

fn item_tag_keys(item: &CrawlItem) -> Vec<String> {
    item.tags.iter().map(tag_key).collect()
}

fn grant_unlocks_tag(g: &Grant, tag: &str, random_tags: &HashMap<String, String>) -> bool {
    match g.kind.as_str() {
        "all" => true,
        "tag" => g.tag.as_deref() == Some(tag),
        "randomTag" => random_tags.get(&g.id).map(String::as_str) == Some(tag),
        _ => false,
    }
}

fn is_visible(
    ent: &Entitlements,
    random_tags: &HashMap<String, String>,
    item_ref: &str,
    tag_keys: &[String],
    first_seen: u64,
    now: u64,
) -> bool {
    if now.saturating_sub(first_seen) < ent.grace_imported_within_ms {
        return true;
    }
    ent.grants
        .iter()
        .filter(|g| g.expires_at > now)
        .any(|g| match g.kind.as_str() {
            "all" => true,
            "item" => g.item_ref.as_deref() == Some(item_ref),
            "sample" => match (&g.seed, g.fraction) {
                (Some(seed), Some(fraction)) => score(seed, item_ref) < fraction,
                _ => false,
            },
            _ => tag_keys
                .iter()
                .any(|t| grant_unlocks_tag(g, t, random_tags)),
        })
}

/// Whether the (signed-in) viewer may see this item's media. Always true when
/// the shop is off.
pub fn is_unlocked(item: &CrawlItem) -> bool {
    with_active(|_, snap| {
        is_visible(
            &snap.ent,
            &snap.random_tags,
            &item_ref(item),
            &item_tag_keys(item),
            item.first_seen,
            now_ms(),
        )
    })
    .unwrap_or(true)
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

const PLACEHOLDER_STYLE: &str = "display:flex;flex-direction:column;align-items:center;\
justify-content:center;gap:0.5em;min-height:8em;padding:1em;background:#2a2a2a;color:#ddd;\
border-radius:4px;text-align:center;";

/// Render `real` media if the item is unlocked, else a locked placeholder.
/// `compact` placeholders (listing thumbnails, which sit inside links) have no
/// buttons; the full placeholder offers an unlock button or a shop link.
pub fn media(item: &CrawlItem, compact: bool, real: Markup) -> Markup {
    let Some(placeholder) = with_active(|shop, snap| {
        if is_visible(
            &snap.ent,
            &snap.random_tags,
            &item_ref(item),
            &item_tag_keys(item),
            item.first_seen,
            now_ms(),
        ) {
            return None;
        }
        Some(locked_placeholder(shop, snap, item, compact))
    }) else {
        return real;
    };
    placeholder.unwrap_or(real)
}

fn locked_placeholder(shop: &Shop, snap: &Snapshot, item: &CrawlItem, compact: bool) -> Markup {
    html! {
        div.shop_locked style=(PLACEHOLDER_STYLE) {
            span { i.fa.fa-lock {} " Locked" }
            @if !compact {
                @if snap.ent.tokens.item > 0 {
                    form method="post" action="/shop/redeem" {
                        input type="hidden" name="kind" value="item";
                        input type="hidden" name="ref" value=(item_ref(item));
                        input type="hidden" name="label" value=(item.title);
                        button type="submit" { "Unlock · 1 view token" }
                    }
                } @else {
                    a href=(shop.config.public_url) { "Visit the shop" }
                }
            }
        }
    }
}

fn format_remaining(ms: u64) -> String {
    let mins = ms / 60_000;
    let (d, h, m) = (mins / 1440, (mins / 60) % 24, mins % 60);
    if d > 0 {
        format!("{}d {}h", d, h)
    } else if h > 0 {
        format!("{}h {}m", h, m)
    } else if m > 0 {
        format!("{}m", m)
    } else {
        "<1m".to_string()
    }
}

/// Header badge with a dropdown of active grants. Empty when the shop is off.
pub fn badge() -> Markup {
    with_active(|shop, snap| {
        let now = now_ms();
        let grants: Vec<&Grant> = snap
            .ent
            .grants
            .iter()
            .filter(|g| g.expires_at > now)
            .collect();
        html! {
            details.shop_badge style="display:inline-block;position:relative;margin-left:1em;" {
                summary style="cursor:pointer;" {
                    (snap.ent.balance) " cr · " (snap.ent.tokens.item) " views · "
                    (grants.len()) " unlocks"
                }
                div style="position:absolute;right:0;z-index:100;min-width:16em;padding:0.5em 1em;background:#222;color:#eee;border:1px solid #444;border-radius:4px;" {
                    @if grants.is_empty() {
                        p { "No active unlocks" }
                    } @else {
                        ul style="padding-left:1em;margin:0.5em 0;" {
                            @for g in &grants {
                                li {
                                    (g.label)
                                    @if let Some(tag) = snap.random_tags.get(&g.id) {
                                        " (" (tag) ")"
                                    }
                                    " · " (format_remaining(g.expires_at - now)) " left"
                                }
                            }
                        }
                    }
                    a href=(shop.config.public_url) { "Open the shop" }
                }
            }
        }
    })
    .unwrap_or_default()
}

/// "Unlock tag" button(s) for a tag page, shown when the shop is on, a tag token
/// is held, and the tag isn't already unlocked. `tag_value` is the tag as tag
/// pages see it; it maps to one key per group it appears in.
pub fn tag_unlock(tag_value: &str, next: &str) -> Markup {
    with_active(|_, snap| {
        if snap.ent.tokens.tag == 0 {
            return html! {};
        }
        let now = now_ms();
        let keys = snap
            .tag_keys_by_value
            .get(tag_value)
            .cloned()
            .unwrap_or_else(|| vec![tag_value.to_string()]);
        let locked: Vec<String> = keys
            .into_iter()
            .filter(|k| {
                !snap
                    .ent
                    .grants
                    .iter()
                    .filter(|g| g.expires_at > now)
                    .any(|g| grant_unlocks_tag(g, k, &snap.random_tags))
            })
            .collect();
        let show_key = locked.len() > 1;
        html! {
            @for key in &locked {
                form.shop_tag_unlock method="post" action="/shop/redeem" style="display:inline-block;margin:0.5em 0.5em 0.5em 0;" {
                    input type="hidden" name="kind" value="tag";
                    input type="hidden" name="tag" value=(key);
                    input type="hidden" name="next" value=(next);
                    button type="submit" {
                        "Unlock tag"
                        @if show_key { " " (key) }
                        " · 1 tag token"
                    }
                }
            }
        }
    })
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// POST /shop/redeem
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RedeemForm {
    kind: String,
    #[serde(default, rename = "ref")]
    item_ref: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    next: Option<String>,
}

/// Only same-origin absolute paths are allowed as redirect targets.
fn safe_path(p: &str) -> Option<&str> {
    (p.starts_with('/') && !p.starts_with("//") && !p.contains('\\')).then_some(p)
}

/// The path (+query) part of a Referer URL.
fn referer_path(referer: &str) -> Option<&str> {
    let rest = referer.split_once("://")?.1;
    Some(rest.find('/').map(|i| &rest[i..]).unwrap_or("/"))
}

pub async fn redeem_handler(req: HttpRequest, form: web::Form<RedeemForm>) -> HttpResponse {
    let form = form.into_inner();
    let Some(shop) = SHOP.get() else {
        return HttpResponse::NotFound().finish();
    };

    let body = match form.kind.as_str() {
        "item" => match &form.item_ref {
            Some(r) => serde_json::json!({
                "kind": "item",
                "ref": r,
                "label": form.label.clone().unwrap_or_default(),
            }),
            None => return HttpResponse::BadRequest().body("missing ref"),
        },
        "tag" => match &form.tag {
            Some(t) => serde_json::json!({ "kind": "tag", "tag": t }),
            None => return HttpResponse::BadRequest().body("missing tag"),
        },
        _ => return HttpResponse::BadRequest().body("unknown kind"),
    };

    let mut post = shop
        .client
        .post(format!("{}/api/v1/redeem", shop.config.url))
        .timeout(REQUEST_TIMEOUT)
        .json(&body);
    if let Some(token) = &shop.config.token {
        post = post.bearer_auth(token);
    }
    match post.send().await {
        Ok(resp) if resp.status().is_success() => {}
        Ok(resp) => log::warn!("Shop redeem returned {}", resp.status()),
        Err(e) => log::warn!("Shop redeem failed: {}", e),
    }
    refresh().await;

    let referer = req
        .headers()
        .get(REFERER)
        .and_then(|v| v.to_str().ok())
        .and_then(referer_path)
        .and_then(safe_path)
        .map(str::to_string);
    let next = form
        .next
        .as_deref()
        .and_then(safe_path)
        .map(str::to_string)
        .or(referer)
        .and_then(|p| HeaderValue::from_str(&p).ok())
        .unwrap_or_else(|| HeaderValue::from_static("/"));
    HttpResponse::SeeOther()
        .insert_header((LOCATION, next))
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn score_test_vector() {
        let s = score("a83f19c2", "alldex:00000000-0000-0000-0000-000000000000");
        assert_eq!(
            hex(&digest("a83f19c2:alldex:00000000-0000-0000-0000-000000000000")[..4]),
            "1ac9d8f5"
        );
        assert!((s - 0x1ac9d8f5 as f64 / 4294967296.0).abs() < 1e-12);
        assert!((s - 0.104642448).abs() < 1e-9);
        assert!((0.10..0.11).contains(&s));
    }

    #[test]
    fn tag_rank_test_vector() {
        assert_eq!(
            hex(&tag_rank("77c1d0e4", "landscape")),
            "16bcdcd919db44629f21387dde4122fd9420299ad80a5f84f9d2076e1bcc89d3"
        );
    }

    #[test]
    fn pick_random_tag_respects_min_items_and_falls_back() {
        let counts: HashMap<String, usize> = [
            ("a".to_string(), 1),
            ("b".to_string(), 5),
            ("c".to_string(), 30),
        ]
        .into_iter()
        .collect();
        assert_eq!(pick_random_tag("seed", 20, &counts), Some("c".to_string()));
        let all = pick_random_tag("seed", 1000, &counts).unwrap();
        let expected = ["a", "b", "c"]
            .into_iter()
            .min_by_key(|t| tag_rank("seed", t))
            .unwrap();
        assert_eq!(all, expected);
        assert_eq!(pick_random_tag("seed", 0, &HashMap::new()), None);
    }

    fn item(key: &str, slug: &str, meta: serde_json::Value, tags: Vec<CrawlTag>) -> CrawlItem {
        let mut item: CrawlItem = serde_json::from_value(serde_json::json!({
            "title": "t", "key": key, "url": "", "description": {"format": "plaintext", "value": ""},
            "meta": meta, "firstSeen": 0, "lastSeen": 0, "seenInLastRefresh": true,
            "tags": [], "files": [],
        }))
        .unwrap();
        item.tags = tags;
        item.site_settings.site_slug = slug.to_string();
        item
    }

    #[test]
    fn item_refs_and_tag_keys() {
        let it = item("k1", "mysite", serde_json::json!({}), vec![]);
        assert_eq!(item_ref(&it), "site:mysite:k1");
        let namespaced = item("mysite/k1", "mysite", serde_json::json!({}), vec![]);
        assert_eq!(item_ref(&namespaced), "site:mysite:k1");
        let alldex = item(
            "k1",
            "mysite",
            serde_json::json!({"shopRef": "alldex:x"}),
            vec![],
        );
        assert_eq!(item_ref(&alldex), "alldex:x");

        assert_eq!(tag_key(&CrawlTag::Simple("cat".into())), "cat");
        assert_eq!(
            tag_key(&CrawlTag::Detailed {
                group: "artist".into(),
                value: "bob".into()
            }),
            "artist:bob"
        );
    }

    fn grant(kind: &str) -> Grant {
        Grant {
            id: format!("g_{}", kind),
            kind: kind.to_string(),
            expires_at: 2_000,
            ..Default::default()
        }
    }

    #[test]
    fn matching_rules() {
        let none = HashMap::new();
        let tags = vec!["cat".to_string(), "artist:bob".to_string()];
        let mut ent = Entitlements {
            enabled: true,
            grace_imported_within_ms: 100,
            ..Default::default()
        };
        // Nothing granted; old item -> locked. Recent import -> visible (grace).
        assert!(!is_visible(&ent, &none, "r", &tags, 0, 1_000));
        assert!(is_visible(&ent, &none, "r", &tags, 950, 1_000));

        ent.grants = vec![Grant {
            item_ref: Some("r".into()),
            ..grant("item")
        }];
        assert!(is_visible(&ent, &none, "r", &tags, 0, 1_000));
        assert!(!is_visible(&ent, &none, "other", &tags, 0, 1_000));
        // Expired by local clock -> ignored.
        assert!(!is_visible(&ent, &none, "r", &tags, 0, 2_000));

        ent.grants = vec![Grant {
            tag: Some("artist:bob".into()),
            ..grant("tag")
        }];
        assert!(is_visible(&ent, &none, "r", &tags, 0, 1_000));
        assert!(!is_visible(
            &ent,
            &none,
            "r",
            &["bob".to_string()],
            0,
            1_000
        ));

        ent.grants = vec![grant("all")];
        assert!(is_visible(&ent, &none, "r", &[], 0, 1_000));

        ent.grants = vec![grant("someFutureKind")];
        assert!(!is_visible(&ent, &none, "r", &tags, 0, 1_000));

        let item_ref = "alldex:00000000-0000-0000-0000-000000000000";
        ent.grants = vec![Grant {
            seed: Some("a83f19c2".into()),
            fraction: Some(0.10),
            ..grant("sample")
        }];
        assert!(!is_visible(&ent, &none, item_ref, &[], 0, 1_000));
        ent.grants[0].fraction = Some(0.11);
        assert!(is_visible(&ent, &none, item_ref, &[], 0, 1_000));

        ent.grants = vec![grant("randomTag")];
        let picked: HashMap<String, String> =
            [("g_randomTag".to_string(), "cat".to_string())].into();
        assert!(is_visible(&ent, &picked, "r", &tags, 0, 1_000));
        assert!(!is_visible(&ent, &picked, "r", &["dog".into()], 0, 1_000));
    }

    #[test]
    fn entitlements_parse_ignores_unknowns() {
        let ent: Entitlements = serde_json::from_value(serde_json::json!({
            "protocol": 1, "enabled": true, "now": 1, "graceImportedWithinMs": 5,
            "balance": 140, "somethingNew": [1, 2],
            "grants": [{"id": "x", "kind": "weird", "createdAt": 1, "expiresAt": 2, "extra": {}}],
            "tokens": {"item": 18, "tag": 1, "itemExpiresAt": 3, "tagExpiresAt": null}
        }))
        .unwrap();
        assert_eq!(ent.balance, 140);
        assert_eq!(ent.tokens.item, 18);
        assert_eq!(ent.grants[0].kind, "weird");
        let disabled: Entitlements =
            serde_json::from_value(serde_json::json!({"enabled": false})).unwrap();
        assert!(!disabled.enabled);
    }

    #[test]
    fn redirect_targets() {
        assert_eq!(safe_path("/aww/r/latest"), Some("/aww/r/latest"));
        assert_eq!(safe_path("//evil.example"), None);
        assert_eq!(safe_path("https://evil.example/"), None);
        assert_eq!(
            referer_path("http://host:8080/aww/r/item/1/2?x=1"),
            Some("/aww/r/item/1/2?x=1")
        );
        assert_eq!(referer_path("http://host"), Some("/"));
    }

    #[test]
    fn off_when_unconfigured() {
        // SHOP is never initialized in tests: everything is visible, no UI.
        let it = item("k", "s", serde_json::json!({}), vec![]);
        assert!(is_unlocked(&it));
        assert_eq!(badge().into_string(), "");
        assert_eq!(tag_unlock("cat", "/").into_string(), "");
        let real = html! { img src="/s/assets/x.jpg" {} };
        assert_eq!(
            media(&it, false, real.clone()).into_string(),
            real.into_string()
        );
    }
}
