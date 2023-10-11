use std::collections::HashMap;

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(crate) struct DevShell {
    pub(crate) packages: Vec<String>,
    pub(crate) env_vars: HashMap<String, String>,
}
