use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EncodingSupport {
    Never,
    TextFiles,
    AllFiles,
}


/// A configuration with the builder interface
#[derive(Clone, Debug)]
pub struct Config {
    pub(crate) text_charset: Option<String>,
    pub(crate) index_files: Vec<String>,
    pub(crate) encoding_support: EncodingSupport,
    pub(crate) content_type: bool,
    pub(crate) etag: bool,
    pub(crate) last_modified: bool,
}

impl Config {
    /// New configuration with default values
    ///
    /// Defaults:
    ///
    /// * `text_charset("utf-8")`
    /// * no index files
    /// * `encodings_on_text_files()`
    /// * etags, last-modified and conditions based on them are enabled
    /// * content-type is enabled
    pub fn new() -> Config {
        Config {
            text_charset: Some(String::from("utf-8")),
            index_files: Vec::new(),
            encoding_support: EncodingSupport::TextFiles,
            content_type: true,
            etag: true,
            last_modified: true,
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

    /// Add a name of the file used as the directory index, like `index.html`
    ///
    /// Multiple names can be added. They are probed in the order in which
    /// they are defined here. Also, these filenames with encoding extensions
    /// are tried too.
    pub fn add_index_file(&mut self, name: &str) -> &mut Self {
        self.index_files.push(String::from(name));
        self
    }

    /// Do not search for `.br` and `.gz` files
    pub fn no_encodings(&mut self) -> &mut Self {
        self.encoding_support = EncodingSupport::Never;
        self
    }

    /// Search for `.br` and `.gz` files for text files
    ///
    /// Text files re those having `text/*` mime type
    /// or `application/javascript`
    pub fn encodings_on_text_files(&mut self) -> &mut Self {
        self.encoding_support = EncodingSupport::TextFiles;
        self
    }

    /// Search for `.br` and `.gz` files for all files regardless of mime type
    pub fn encodings_on_all_files(&mut self) -> &mut Self {
        self.encoding_support = EncodingSupport::AllFiles;
        self
    }
    /// Togggles generation of Content-Type header (so user can override)
    ///
    /// By default it's enabled
    pub fn content_type(&mut self, value: bool) -> &mut Self {
        self.content_type = value;
        self
    }
    /// Toggles generation of Etag generation (and so `If-None-Match` too)
    ///
    /// By default it's enabled
    pub fn etag(&mut self, value: bool) -> &mut Self {
        self.etag = value;
        self
    }

    /// Toggles generation of Last-Modified (and so `If-Modified-Since` too)
    ///
    /// Note: Last-Modified date is never sent if date is earlier than
    /// 1990-01-01.
    pub fn last_modified(&mut self, value: bool) -> &mut Self {
        self.last_modified = value;
        self
    }

    /// Finalize configuration and wrap into an Arc
    pub fn done(&self) -> Arc<Config> {
        Arc::new(self.clone())
    }
}
