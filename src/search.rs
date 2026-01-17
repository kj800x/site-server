//! S-expression search parser and evaluator.
//!
//! NOTE: When adding or modifying search functions in this file, you must also
//! update the documentation and examples in `src/handlers/search.rs` (the search
//! form tooltip that shows available functions and examples to users).

use crate::reprocessors::{extract_text_from_formatted_text, search_json_value_recursive};
use crate::site::{CrawlItem, FileCrawlType};
use crate::timestring;
use chrono::Utc;
use chrono_tz::America::New_York;
use chrono_tz::Tz;

/// The timezone used for interpreting time strings in search queries.
const SEARCH_TIMEZONE: Tz = New_York;

#[derive(Debug, Clone)]
pub enum SearchExpr {
    And(Vec<SearchExpr>),
    Or(Vec<SearchExpr>),
    Not(Box<SearchExpr>),
    Tag(String),
    Type(String), // "image", "video", or "text"
    Site(String), // matches item.site_settings.site_slug
    Fulltext(String),
    Title(String),
    Meta(String),
    Desc(String),
    Url(String),
    After(String),  // Flexible time string
    Before(String), // Flexible time string
    During(String), // Flexible time string (must be a range)
}

#[derive(Debug, Clone)]
pub enum ParseError {
    UnexpectedToken(String),
    UnexpectedEnd,
    InvalidFunction(String),
    InvalidArgument(String),
    InvalidTimestamp(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedToken(t) => write!(f, "Unexpected token: {}", t),
            ParseError::UnexpectedEnd => write!(f, "Unexpected end of input"),
            ParseError::InvalidFunction(fn_name) => write!(f, "Invalid function: {}", fn_name),
            ParseError::InvalidArgument(arg) => write!(f, "Invalid argument: {}", arg),
            ParseError::InvalidTimestamp(ts) => write!(f, "Invalid timestamp: {}", ts),
        }
    }
}

impl std::error::Error for ParseError {}

pub fn parse_search_expr(input: &str) -> Result<SearchExpr, ParseError> {
    let tokens = tokenize(input)?;
    let (expr, remaining_pos) = parse_expr(&tokens, 0)?;
    if remaining_pos < tokens.len() {
        return Err(ParseError::UnexpectedToken(format!(
            "Unexpected tokens after expression: {:?}",
            &tokens[remaining_pos..]
        )));
    }
    Ok(expr)
}

#[derive(Debug, Clone)]
enum Token {
    OpenParen,
    CloseParen,
    String(String),
}

fn tokenize(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current_string = String::new();
    let mut in_string = false;
    let mut escape = false;

    while let Some(ch) = chars.next() {
        if escape {
            current_string.push(ch);
            escape = false;
            continue;
        }

        match ch {
            '\\' if in_string => {
                escape = true;
            }
            '"' => {
                if in_string {
                    tokens.push(Token::String(current_string.clone()));
                    current_string.clear();
                    in_string = false;
                } else {
                    in_string = true;
                }
            }
            '(' if !in_string => {
                if !current_string.trim().is_empty() {
                    tokens.push(Token::String(current_string.trim().to_string()));
                    current_string.clear();
                }
                tokens.push(Token::OpenParen);
            }
            ')' if !in_string => {
                if !current_string.trim().is_empty() {
                    tokens.push(Token::String(current_string.trim().to_string()));
                    current_string.clear();
                }
                tokens.push(Token::CloseParen);
            }
            ch if in_string => {
                current_string.push(ch);
            }
            ch if ch.is_whitespace() && !in_string => {
                if !current_string.trim().is_empty() {
                    tokens.push(Token::String(current_string.trim().to_string()));
                    current_string.clear();
                }
            }
            ch => {
                current_string.push(ch);
            }
        }
    }

    if in_string {
        return Err(ParseError::UnexpectedEnd);
    }

    if !current_string.trim().is_empty() {
        tokens.push(Token::String(current_string.trim().to_string()));
    }

    Ok(tokens)
}

fn parse_expr(tokens: &[Token], start: usize) -> Result<(SearchExpr, usize), ParseError> {
    if start >= tokens.len() {
        return Err(ParseError::UnexpectedEnd);
    }

    match &tokens[start] {
        Token::OpenParen => {
            let mut pos = start + 1;
            if pos >= tokens.len() {
                return Err(ParseError::UnexpectedEnd);
            }

            let function_name = match &tokens[pos] {
                Token::String(s) => {
                    pos += 1;
                    s.clone()
                }
                _ => return Err(ParseError::UnexpectedToken(format!("{:?}", tokens[pos]))),
            };

            let function_name_lower = function_name.to_lowercase();

            match function_name_lower.as_str() {
                "and" => {
                    let mut args = Vec::new();
                    while pos < tokens.len() {
                        if let Token::CloseParen = tokens[pos] {
                            pos += 1;
                            break;
                        }
                        let (expr, new_pos) = parse_expr(tokens, pos)?;
                        args.push(expr);
                        pos = new_pos;
                    }
                    if args.is_empty() {
                        return Err(ParseError::InvalidArgument(
                            "and requires at least one argument".to_string(),
                        ));
                    }
                    Ok((SearchExpr::And(args), pos))
                }
                "or" => {
                    let mut args = Vec::new();
                    while pos < tokens.len() {
                        if let Token::CloseParen = tokens[pos] {
                            pos += 1;
                            break;
                        }
                        let (expr, new_pos) = parse_expr(tokens, pos)?;
                        args.push(expr);
                        pos = new_pos;
                    }
                    if args.is_empty() {
                        return Err(ParseError::InvalidArgument(
                            "or requires at least one argument".to_string(),
                        ));
                    }
                    Ok((SearchExpr::Or(args), pos))
                }
                "not" => {
                    let (expr, new_pos) = parse_expr(tokens, pos)?;
                    if new_pos >= tokens.len() || !matches!(tokens[new_pos], Token::CloseParen) {
                        return Err(ParseError::InvalidArgument(
                            "not requires exactly one argument".to_string(),
                        ));
                    }
                    pos = new_pos + 1;
                    Ok((SearchExpr::Not(Box::new(expr)), pos))
                }
                "tag" | "type" | "site" | "fulltext" | "title" | "meta" | "desc" | "url"
                | "after" | "before" | "during" => {
                    if pos >= tokens.len() {
                        return Err(ParseError::UnexpectedEnd);
                    }
                    let arg = match &tokens[pos] {
                        Token::String(s) => s.clone(),
                        Token::OpenParen => {
                            return Err(ParseError::InvalidArgument(format!(
                                "{} requires a string argument",
                                function_name
                            )));
                        }
                        Token::CloseParen => {
                            return Err(ParseError::InvalidArgument(format!(
                                "{} requires an argument",
                                function_name
                            )));
                        }
                    };
                    pos += 1;

                    if pos >= tokens.len() || !matches!(tokens[pos], Token::CloseParen) {
                        return Err(ParseError::InvalidArgument(format!(
                            "{} requires exactly one argument",
                            function_name
                        )));
                    }
                    pos += 1;

                    let expr = match function_name_lower.as_str() {
                        "tag" => SearchExpr::Tag(arg),
                        "type" => {
                            let type_lower = arg.to_lowercase();
                            if type_lower != "image"
                                && type_lower != "video"
                                && type_lower != "text"
                            {
                                return Err(ParseError::InvalidArgument(format!(
                                    "type must be 'image', 'video', or 'text', got: {}",
                                    arg
                                )));
                            }
                            SearchExpr::Type(type_lower)
                        }
                        "site" => SearchExpr::Site(arg),
                        "fulltext" => SearchExpr::Fulltext(arg),
                        "title" => SearchExpr::Title(arg),
                        "meta" => SearchExpr::Meta(arg),
                        "desc" => SearchExpr::Desc(arg),
                        "url" => SearchExpr::Url(arg),
                        "after" => {
                            // Validate the time string can be parsed
                            let now = Utc::now().with_timezone(&SEARCH_TIMEZONE);
                            if timestring::parse(&arg, now, SEARCH_TIMEZONE).is_err() {
                                return Err(ParseError::InvalidTimestamp(arg));
                            }
                            SearchExpr::After(arg)
                        }
                        "before" => {
                            // Validate the time string can be parsed
                            let now = Utc::now().with_timezone(&SEARCH_TIMEZONE);
                            if timestring::parse(&arg, now, SEARCH_TIMEZONE).is_err() {
                                return Err(ParseError::InvalidTimestamp(arg));
                            }
                            SearchExpr::Before(arg)
                        }
                        "during" => {
                            // Validate the time string can be parsed AND is a range
                            let now = Utc::now().with_timezone(&SEARCH_TIMEZONE);
                            match timestring::parse(&arg, now, SEARCH_TIMEZONE) {
                                Ok(spec) if spec.is_range() => SearchExpr::During(arg),
                                Ok(_) => {
                                    return Err(ParseError::InvalidArgument(format!(
                                        "during requires a time range, not a specific moment: {}",
                                        arg
                                    )));
                                }
                                Err(_) => {
                                    return Err(ParseError::InvalidTimestamp(arg));
                                }
                            }
                        }
                        _ => unreachable!(),
                    };
                    Ok((expr, pos))
                }
                _ => Err(ParseError::InvalidFunction(function_name)),
            }
        }
        Token::String(s) => Err(ParseError::UnexpectedToken(format!(
            "Unexpected string token at top level: {}",
            s
        ))),
        Token::CloseParen => Err(ParseError::UnexpectedToken("Unexpected ')'".to_string())),
    }
}

pub fn evaluate_search_expr(expr: &SearchExpr, item: &CrawlItem) -> bool {
    match expr {
        SearchExpr::And(exprs) => exprs.iter().all(|e| evaluate_search_expr(e, item)),
        SearchExpr::Or(exprs) => exprs.iter().any(|e| evaluate_search_expr(e, item)),
        SearchExpr::Not(expr) => !evaluate_search_expr(expr, item),
        SearchExpr::Tag(tag) => {
            let tag_lower = tag.to_lowercase();
            item.tags
                .iter()
                .any(|t| t.to_string().to_lowercase() == tag_lower)
        }
        SearchExpr::Type(file_type) => {
            let flat_files = item.flat_files();
            match file_type.as_str() {
                "image" => flat_files.values().any(|f| f.is_image()),
                "video" => flat_files.values().any(|f| f.is_video()),
                "text" => flat_files.values().any(|f| f.is_text()),
                _ => false,
            }
        }
        SearchExpr::Site(site_slug) => item.site_settings.site_slug == *site_slug,
        SearchExpr::Fulltext(search_text) => {
            let search_lower = search_text.to_lowercase();

            // Search in title
            if item.title.to_lowercase().contains(&search_lower) {
                return true;
            }

            // Search in URL
            if item.url.to_lowercase().contains(&search_lower) {
                return true;
            }

            // Search in description
            let desc_text = extract_text_from_formatted_text(&item.description);
            if desc_text.to_lowercase().contains(&search_lower) {
                return true;
            }

            // Search in meta
            if search_json_value_recursive(&item.meta, search_text) {
                return true;
            }

            // Search in text file content
            let flat_files = item.flat_files();
            for file in flat_files.values() {
                if let FileCrawlType::Text { content, .. } = file {
                    if content.to_lowercase().contains(&search_lower) {
                        return true;
                    }
                }
            }

            false
        }
        SearchExpr::Title(search_text) => item
            .title
            .to_lowercase()
            .contains(&search_text.to_lowercase()),
        SearchExpr::Meta(search_text) => search_json_value_recursive(&item.meta, search_text),
        SearchExpr::Desc(search_text) => {
            let desc_text = extract_text_from_formatted_text(&item.description);
            desc_text
                .to_lowercase()
                .contains(&search_text.to_lowercase())
        }
        SearchExpr::Url(search_text) => item
            .url
            .to_lowercase()
            .contains(&search_text.to_lowercase()),
        SearchExpr::After(time_str) => {
            let now = Utc::now().with_timezone(&SEARCH_TIMEZONE);
            let spec = timestring::parse(time_str, now, SEARCH_TIMEZONE)
                .expect("Time string should be validated during parsing");
            let threshold = spec.for_after();
            item.source_published >= threshold
        }
        SearchExpr::Before(time_str) => {
            let now = Utc::now().with_timezone(&SEARCH_TIMEZONE);
            let spec = timestring::parse(time_str, now, SEARCH_TIMEZONE)
                .expect("Time string should be validated during parsing");
            let threshold = spec.for_before();
            item.source_published <= threshold
        }
        SearchExpr::During(time_str) => {
            let now = Utc::now().with_timezone(&SEARCH_TIMEZONE);
            let spec = timestring::parse(time_str, now, SEARCH_TIMEZONE)
                .expect("Time string should be validated during parsing");
            spec.contains(item.source_published)
        }
    }
}
