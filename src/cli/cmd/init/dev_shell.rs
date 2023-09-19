use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct DevShell {
    pub(super) packages: Vec<String>,
    pub(super) env_vars: HashMap<String, String>,
}
