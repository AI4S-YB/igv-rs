use thiserror::Error;

#[derive(Error, Debug)]
pub enum RenderError {
    #[error("usvg parse: {0}")]
    UsvgParse(String),
    #[error("png encode: {0}")]
    PngEncode(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
