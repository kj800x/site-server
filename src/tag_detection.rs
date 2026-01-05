use std::{collections::HashMap, f64::consts::E};

use crate::{
    site::{CrawlItem, FormattedText},
    workdir::WorkDir,
};

trait Stemmer {
    fn stem(&self, word: &str) -> Option<String>;
}

struct HardcodedStemmer {
    map: HashMap<String, String>,
}

impl Stemmer for HardcodedStemmer {
    fn stem(&self, word: &str) -> Option<String> {
        let v: Vec<Box<dyn Fn() -> Option<String>>> = vec![
            Box::new(|| Some(word.to_string())),
            Box::new(|| {
                if word.ends_with("s") {
                    Some(word.trim_end_matches("s").to_string())
                } else {
                    None
                }
            }),
            Box::new(|| {
                if word.ends_with("re") {
                    Some(word.trim_end_matches("re").to_string())
                } else {
                    None
                }
            }),
            Box::new(|| {
                if word.ends_with("nt") {
                    Some(word.trim_end_matches("nt").to_string())
                } else {
                    None
                }
            }),
            Box::new(|| {
                if word.ends_with("ve") {
                    Some(word.trim_end_matches("ve").to_string())
                } else {
                    None
                }
            }),
            Box::new(|| {
                if word.ends_with("d") {
                    Some(word.trim_end_matches("d").to_string())
                } else {
                    None
                }
            }),
            Box::new(|| {
                if word.ends_with("ll") {
                    Some(word.trim_end_matches("ll").to_string())
                } else {
                    None
                }
            }),
        ];

        for f in v {
            let result = f().and_then(|r| self.map.get(&r).cloned());
            if let Some(result) = result {
                return Some(result);
            }
        }

        None
    }
}

impl HardcodedStemmer {
    fn new() -> Self {
        const STEMMER_BYTES: &[u8] = include_bytes!("data/now-stemming.txt");
        let mut map: HashMap<String, String> = HashMap::new();

        let stemmer_text = String::from_utf8_lossy(STEMMER_BYTES);
        for line in stemmer_text.lines() {
            // split on tab character, the first one is the stem and all others on the line are the original words
            let parts = line.split('\t').collect::<Vec<&str>>();
            let stem = parts[0];
            let original_words = parts[1..].to_vec();
            for original_word in original_words {
                if let Some(existing_stem) = map.get(original_word) {
                    map.insert(
                        original_word.to_string(),
                        if existing_stem.len() < stem.len() {
                            existing_stem.to_string()
                        } else {
                            stem.to_string()
                        },
                    );
                } else {
                    map.insert(original_word.to_string(), stem.to_string());
                }
            }
        }

        Self { map }
    }
}

pub trait TagDetect {
    fn tag_detect(&self);
}

// Extract potential grouping tags from items' titles and descriptions.
// Uses document frequency filtering to find meaningful terms that group items:
// - Terms must appear in multiple items (min_documents) to be grouping tags
// - Terms must not appear in too many items (max_document_percent) to avoid generic terms
// - Scores by total term frequency across all documents
impl TagDetect for WorkDir {
    fn tag_detect(&self) {
        let items = &self.crawled.items;
        let total_items = items.len();

        if total_items == 0 {
            println!("No items found in workdir");
            return;
        }

        // Configuration: filter bounds for meaningful grouping tags
        let min_documents = 3; // Must appear in at least 5 items
        let max_document_percent = 0.7; // Must not appear in more than 40% of items
        let max_documents = (total_items as f64 * max_document_percent) as usize;
        // values above 8.0 should be boosted (meaningful), whereas values below 8.0 should be downweighted
        let corpus_idf_scale_point = 8.0;
        // Controls how much the scaling function applies
        let corpus_idf_scaling = 1.0;
        // Default IDF score for words not found in the english corpus (domain specific words)
        // Score of 10 means treat these as meaningful.
        let corpus_unknown_word_idf_score = 10.0;

        let stemmer = HardcodedStemmer::new();

        // Extract text from each item (title + description)
        let mut item_texts: Vec<String> = Vec::new();
        for (_key, item) in items.iter() {
            let text = extract_text_from_item(item);
            item_texts.push(text);
        }

        // Calculate term frequencies and document frequencies
        // Track stems for grouping, but also track original word forms
        let mut total_term_freqs: HashMap<String, usize> = HashMap::new();
        let mut document_freqs: HashMap<String, usize> = HashMap::new();
        // Map from stem -> (original word -> frequency)
        let mut stem_to_originals: HashMap<String, HashMap<String, usize>> = HashMap::new();

        for text in &item_texts {
            let tokens = tokenize(text, &stemmer);
            let mut doc_terms: std::collections::HashSet<String> = std::collections::HashSet::new();

            // Count term frequency in this document
            for (stem, original) in &tokens {
                *total_term_freqs.entry(stem.clone()).or_insert(0) += 1;
                doc_terms.insert(stem.clone());

                // Track original word forms for each stem
                *stem_to_originals
                    .entry(stem.clone())
                    .or_insert_with(HashMap::new)
                    .entry(original.clone())
                    .or_insert(0) += 1;
            }

            // Track document frequency (how many documents contain each term)
            for term in doc_terms {
                *document_freqs.entry(term).or_insert(0) += 1;
            }
        }

        // Filter stop words and score terms with corpus-based IDF weighting
        let mut candidate_tags: Vec<(String, f64, f64, Option<usize>, f64, usize, usize)> =
            Vec::new();
        let stop_words = get_stop_words(&stemmer);

        // Load corpus IDF map from embedded file (stems words to match tokenized terms)
        let corpus_idf = load_corpus_idf_map(&stemmer);

        let total_items_f64 = total_items as f64;

        for (term, &total_freq) in total_term_freqs.iter() {
            // Filter out stop words
            if stop_words.contains(term) {
                continue;
            }

            let df = document_freqs.get(term).copied().unwrap_or(0);

            // Filter: must appear in multiple documents but not too many
            if df >= min_documents && df <= max_documents {
                // Terms are already stemmed during tokenization, so we can look them up directly
                // This ensures "eyes" maps to "eye" and "favorites" maps to "favorite" (Porter stemmer)
                // Use corpus-based IDF: log(corpus_size / estimated_docs_with_term_in_corpus)
                // This down-weights common English words (like "video", "fun")
                // while keeping domain-specific terms (like "exclusive", "birthday") higher
                // Default to 10.0 makes words that never appear in the global corpus (i.e. domain specific words) get boosted
                // Tune this as needed.
                let corpus_idf_score = corpus_idf.get(term).copied();

                // Also consider local document frequency for additional weighting
                let local_idf = if df > 0 {
                    (total_items_f64 / df as f64).ln()
                } else {
                    0.0
                };

                let corpus_idf_scaled = E.powf(
                    corpus_idf_scaling
                        * (corpus_idf_score
                            .map(|f| f.0)
                            .unwrap_or(corpus_unknown_word_idf_score)
                            - corpus_idf_scale_point),
                );

                // Combine corpus IDF (down-weights common English) with local IDF (favors distinctive terms in corpus)
                // Weight corpus IDF more heavily to prioritize down-weighting common English words
                let combined_idf = corpus_idf_scaled * 0.7 + local_idf * 0.3;

                // Score: total frequency weighted by combined IDF
                let score = total_freq as f64 * combined_idf;
                candidate_tags.push((
                    term.clone(),
                    score,
                    corpus_idf_scaled,
                    corpus_idf_score.map(|f| f.1),
                    local_idf,
                    df,
                    total_freq,
                ));
            }
        }

        // Sort by score descending (most frequent terms that meet criteria)
        candidate_tags.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Output results: term, score (IDF-weighted), total_frequency, document_count, documents_not_including
        println!(
            "\nPotential grouping tags (appearing in {} to {} documents, stop words filtered):\n",
            min_documents, max_documents
        );

        // Print header
        println!(
            "term\t\t|stem\t\t| score  \t| global_idf \t| global_rank\t| corp_idf \t| doc_percent | occ_count"
        );
        println!("{}", "-".repeat(100));

        // Print rows
        for (stem, score, global_idf, global_rank, corp_idf, doc_count, occ_count) in candidate_tags
        {
            // Find the most common original word form for this stem
            let display_term = stem_to_originals
                .get(&stem)
                .and_then(|originals| {
                    originals
                        .iter()
                        .max_by_key(|(_, &freq)| freq)
                        .map(|(word, _)| word.clone())
                })
                .unwrap_or_else(|| stem.clone());

            let doc_percent = doc_count as f64 / total_items as f64;

            // Calculate tabs: 2 tabs if term <= 8 chars, 1 tab if > 8 chars
            let tabs = if display_term.len() < 8 { "\t\t" } else { "\t" };
            let stem_tabs = if stem.len() < 6 { "\t\t" } else { "\t" };

            println!(
                "{}{}| {}{}| {:.2}  \t| {:.2}  \t| {:?}{} \t| {:.2}  \t| {:.2}% \t| {}",
                display_term,
                tabs,
                stem,
                stem_tabs,
                score,
                global_idf,
                global_rank,
                if let None = global_rank { "\t" } else { "" },
                corp_idf,
                doc_percent * 100.0,
                occ_count
            );
        }
        println!();
    }
}

/// Extract text content from a CrawlItem (title + description)
fn extract_text_from_item(item: &CrawlItem) -> String {
    let mut text = item.title.clone();

    // Extract text from FormattedText description
    let desc_text = match &item.description {
        FormattedText::Markdown { value } => value.clone(),
        FormattedText::Plaintext { value } => value.clone(),
        FormattedText::Html { value } => value.clone(),
    };

    if !desc_text.is_empty() {
        text.push(' ');
        text.push_str(&desc_text);
    }

    text
}

/// Strip HTML tags by replacing them with spaces
fn strip_html_tags(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                result.push(' '); // Replace tag with space
            }
            _ => {
                if !in_tag {
                    result.push(c);
                }
            }
        }
    }

    result
}

/// Tokenize text into lowercase stemmed terms, removing punctuation and HTML tags
/// Returns a vector of (stem, original_word) pairs
/// Stems tokens so that "favorite" and "favorites" become the same term
/// Also normalizes irregular verb forms like "made" -> "make"
fn tokenize(text: &str, stemmer: &impl Stemmer) -> Vec<(String, String)> {
    // First strip HTML tags
    let cleaned = strip_html_tags(text);

    cleaned
        .split_whitespace()
        .map(|word| {
            // Remove punctuation and convert to lowercase
            let token: String = word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            token
        })
        .filter(|token| {
            // Filter out empty tokens, single characters, and pure numbers
            !token.is_empty() && token.len() > 1 && !token.chars().all(|c| c.is_ascii_digit())
        })
        .map(|token| (stemmer.stem(&token).unwrap_or_else(|| token.clone()), token))
        .collect()
}

/// Load corpus-based IDF scores from embedded english_corpus.txt file
/// The file contains words ordered by frequency (most frequent first)
/// Words are stemmed to match the tokenization process
/// Frequency approximation: 1/n where n is the line number (1-indexed)
/// IDF is calculated as log(line_number), where common words (low line numbers) get lower IDF
/// and rare words (high line numbers) get higher IDF
/// Words not in the corpus get default IDF of 1.0 (neutral weighting)
fn load_corpus_idf_map(stemmer: &impl Stemmer) -> HashMap<String, (f64, usize)> {
    const CORPUS_BYTES: &[u8] = include_bytes!("data/english_corpus.txt");

    let mut idf_map = HashMap::<String, (f64, usize)>::new();

    // Parse the corpus file line by line
    // Each line is a word, ordered by frequency (most frequent = line 1)
    let corpus_text = String::from_utf8_lossy(CORPUS_BYTES);

    for (line_num, line) in corpus_text.lines().enumerate() {
        let word = line.trim().to_lowercase();

        // Skip empty lines
        if word.is_empty() {
            continue;
        }

        let stemmed_word = stemmer.stem(&word).unwrap_or_else(|| word.clone());

        // Line numbers are 0-indexed in enumerate, so add 1 for 1-indexed position
        let position = (line_num + 1) as f64;

        // IDF = log(position)
        // Common words (low position) get lower IDF
        // Rare words (high position) get higher IDF
        // Use the minimum IDF if multiple words stem to the same form (take the most common one)
        let idf = position.ln();
        idf_map
            .entry(stemmed_word)
            .and_modify(|existing_idf| {
                // If multiple words stem to the same form, keep the lower IDF (more common word)
                if idf < existing_idf.0 {
                    *existing_idf = (idf, line_num + 1);
                }
            })
            .or_insert((idf, line_num + 1));
    }

    idf_map
}

/// Get a set of common English stop words to filter out
/// Loads from embedded stop_words.txt file and stems them to match stemmed tokens
fn get_stop_words(stemmer: &impl Stemmer) -> std::collections::HashSet<String> {
    const STOP_WORDS_BYTES: &[u8] = include_bytes!("data/stop_words.txt");

    let stop_words_text = String::from_utf8_lossy(STOP_WORDS_BYTES);

    stop_words_text
        .lines()
        .map(|line| line.trim().to_lowercase())
        .filter(|word| !word.is_empty())
        .map(|word| stemmer.stem(&word).unwrap_or_else(|| word.clone()))
        .collect()
}
