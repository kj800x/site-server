use error_chain::error_chain;
error_chain! {
    foreign_links {
        Io(::std::io::Error);
        Serde(::serde_json::Error);
    }
}
