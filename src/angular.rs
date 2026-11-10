//! A resolved import: no list of known Angular attributes/pipes is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub symbol: String,
    pub module: String,
    /// A module already in Component.imports may export this declaration.
    pub covered_by: Option<String>,
}

impl Dependency {
    pub fn new(symbol: impl Into<String>, module: impl Into<String>, covered_by: Option<&str>) -> Self {
        Self { symbol: symbol.into(), module: module.into(), covered_by: covered_by.map(str::to_owned) }
    }
}
