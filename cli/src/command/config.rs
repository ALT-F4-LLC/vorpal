use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct VorpalConfigSourceGo {
    pub directory: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VorpalConfigSourceRust {
    pub bin: Option<String>,
    pub packages: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VorpalConfigSource {
    pub go: Option<VorpalConfigSourceGo>,
    pub includes: Option<Vec<String>>,
    pub rust: Option<VorpalConfigSourceRust>,
    pub script: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VorpalConfig {
    pub environments: Option<Vec<String>>,
    pub language: Option<String>,
    pub name: Option<String>,
    pub source: Option<VorpalConfigSource>,
}
