use std::sync::Arc;


#[derive(Clone, Debug)]
pub struct Config {
    pub(crate) text_charset: Option<String>,
}

impl Config {
    /// New configuration with default values
    ///
    /// Defaults:
    ///
    /// * `text_charset("utf-8")`
    pub fn new() -> Config {
        Config {
            text_charset: Some(String::from("utf-8")),
        }
    }
    /// Set default charset for all text mime types
    ///
    /// Note: by default it's `utf-8`, you may disable it using
    /// `no_text_charset()`
    pub fn text_charset(&mut self, charset: &str) -> &mut Self {
        self.text_charset = Some(charset.into());
        self
    }
    /// Disable adding charset value to all text mime types
    pub fn no_text_charset(&mut self) -> &mut Self {
        self.text_charset = None;
        self
    }
    pub fn done(&self) -> Arc<Config> {
        Arc::new(self.clone())
    }
}
