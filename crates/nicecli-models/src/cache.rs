use crate::{ModelCatalogError, StaticModelsJson};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub(crate) fn load_catalog_from_path(
    path: &Path,
) -> Result<Option<StaticModelsJson>, ModelCatalogError> {
    match fs::read(path) {
        Ok(bytes) => crate::parse_catalog_bytes(&bytes).map(Some),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ModelCatalogError::Io(error)),
    }
}

pub(crate) fn save_catalog_to_path(
    path: &Path,
    catalog: &StaticModelsJson,
) -> Result<(), ModelCatalogError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(ModelCatalogError::Io)?;
    }
    let bytes = serde_json::to_vec_pretty(catalog)?;
    fs::write(path, bytes).map_err(ModelCatalogError::Io)?;
    Ok(())
}
