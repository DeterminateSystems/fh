#[derive(Debug, thiserror::Error)]
pub(crate) enum FhError {
    #[error("Nix command failed; check prior Nix output for details")]
    FailedNixCommand,

    #[error("file error: {0}")]
    Filesystem(#[from] std::io::Error),

    #[error("flake name parsing error: {0}")]
    FlakeParse(String),

    #[error("invalid header: {0}")]
    Header(#[from] reqwest::header::InvalidHeaderValue),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("interactive initializer error: {0}")]
    Interactive(#[from] inquire::InquireError),

    #[error("json parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("label parsing error: {0}")]
    LabelParse(String),

    #[error("malformed output reference: {0}")]
    MalformedOutputRef(String),

    #[error("malformed flake reference")]
    MalformedFlakeOutputRef,

    #[error("{0} is not installed or not on the PATH")]
    MissingExecutable(String),

    #[error("missing from flake output reference: {0}")]
    MissingFromOutputRef(String),

    #[error("the flake has no inputs")]
    NoInputs,

    #[error("template error: {0}")]
    Render(#[from] handlebars::RenderError),

    #[error(transparent)]
    Report(#[from] color_eyre::Report),

    #[error("template error: {0}")]
    Template(#[from] Box<handlebars::TemplateError>),

    #[error("a presumably unreachable point was reached: {0}")]
    Unreachable(String),

    #[error("url parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("xdg base directory error: {0}")]
    Xdg(#[from] xdg::BaseDirectoriesError),
}
