use crate::{
    core::{
        model::to_yaml,
        store::MODEL_NAMESPACE,
        util::{get_type_name, parse_debug_name},
    },
    uniffi::{
        error::{Result, selector},
        model::Annotation,
        store::{ModelID, ModelInfo, filestore::LocalFileStore},
    },
};
use colored::Colorize as _;
use glob::glob;
use heck::ToSnakeCase as _;
use regex::Regex;
use serde::{Serialize, de::DeserializeOwned};
use serde_yaml;
use snafu::OptionExt as _;
use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};

#[expect(clippy::expect_used, reason = "Valid static regex")]
static RE_MODEL_METADATA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
            ^
                (?<store_directory>.*)\/
                    (?<namespace>[a-z_]+)\/
                        (?<class>[a-z_]+)\/
                            (?<hash>[0-9a-f]+)\/
                                (
                                    annotation\/
                                        (?<name>[0-9a-zA-Z\-]+)
                                        -
                                        (?<version>[0-9]+\.[0-9]+\.[0-9]+)
                                        \.yaml
                                |
                                    spec\.yaml
                                )
            $
            ",
    )
    .expect("Invalid model metadata regex.")
});

impl LocalFileStore {
    /// Relative path where model specification is stored within the model directory.
    pub const SPEC_RELPATH: &str = "spec.yaml";
    /// Relative path where model annotation is stored within the model directory.
    pub fn make_annotation_relpath(name: &str, version: &str) -> PathBuf {
        PathBuf::from(format!("annotation/{name}-{version}.yaml"))
    }
    /// Build the storage path with the model directory (`hash`) and a file's relative path.
    pub fn make_path<T: fmt::Debug>(
        &self,
        model: &T,
        hash: &str,
        relpath: impl AsRef<Path>,
    ) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}/{}/{}",
            self.directory.to_string_lossy(),
            MODEL_NAMESPACE,
            parse_debug_name(model).to_snake_case(),
            hash
        ))
        .join(relpath)
    }

    fn find_model_metadata(glob_pattern: &Path) -> Result<impl Iterator<Item = ModelInfo>> {
        let paths = glob(&glob_pattern.to_string_lossy())?.filter_map(move |filepath| {
            let filepath_string = String::from(filepath.ok()?.to_string_lossy());
            let group = RE_MODEL_METADATA.captures(&filepath_string)?;
            Some(ModelInfo {
                name: group.name("name").map(|name| name.as_str().to_owned()),
                version: group
                    .name("version")
                    .map(|version| version.as_str().to_owned()),
                hash: group["hash"].to_string(),
            })
        });
        Ok(paths)
    }
    /// Find hash using name and version.
    ///
    /// # Errors
    ///
    /// Will return error if unable to find.
    pub(crate) fn lookup_hash<T: fmt::Debug>(
        &self,
        model: &T,
        name: &str,
        version: &str,
    ) -> Result<String> {
        let model_info = Self::find_model_metadata(&self.make_path(
            model,
            "*",
            Self::make_annotation_relpath(name, version),
        ))?
        .next()
        .context(selector::NoAnnotationFound {
            class: parse_debug_name(model).to_snake_case(),
            name: name.to_owned(),
            version: version.to_owned(),
        })?;
        Ok(model_info.hash)
    }

    pub(crate) fn save_file(file: impl AsRef<Path>, content: impl AsRef<[u8]>) -> Result<()> {
        if let Some(parent) = file.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(file, content)?;
        Ok(())
    }
    /// How any model is stored.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue storing the model.
    pub(crate) fn save_model<T: Serialize + fmt::Debug>(
        &self,
        model: &T,
        hash: &str,
        annotation: Option<&Annotation>,
    ) -> Result<()> {
        let class = get_type_name::<T>().to_snake_case();
        // Save annotation if defined and doesn't collide globally i.e. model, name, version
        if let Some(provided_annotation) = annotation {
            let relpath = &Self::make_annotation_relpath(
                &provided_annotation.name,
                &provided_annotation.version,
            );
            if let Some((found_hash, found_name, found_version)) =
                Self::find_model_metadata(&self.make_path(model, "*", relpath))?
                    .next()
                    .and_then(|model_info| {
                        Some((model_info.hash, model_info.name?, model_info.version?))
                    })
            {
                println!(
                    "{}",
                    format!(
                        "Skip saving {} annotation since `{}`, `{}`, `{}` exists.",
                        class.bright_cyan(),
                        found_hash.bright_cyan(),
                        found_name.bright_cyan(),
                        found_version.bright_cyan(),
                    )
                    .yellow(),
                );
            } else {
                Self::save_file(
                    self.make_path(model, hash, relpath),
                    serde_yaml::to_string(provided_annotation)?,
                )?;
            }
        }
        // Save model specification and skip if it already exist e.g. on new annotations
        let spec_file = &self.make_path(model, hash, Self::SPEC_RELPATH);
        if spec_file.exists() {
            println!(
                "{}",
                format!(
                    "Skip saving {} model since `{}` exists.",
                    class.bright_cyan(),
                    hash.bright_cyan(),
                )
                .yellow(),
            );
        } else {
            Self::save_file(spec_file, to_yaml(model)?)?;
        }
        Ok(())
    }
    /// How to load any stored model into an instance.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue loading the model from the store using `name` and
    /// `version`.
    pub(crate) fn load_model<T: DeserializeOwned + Default + fmt::Debug>(
        &self,
        model_id: &ModelID,
    ) -> Result<(T, Option<Annotation>, String)> {
        match model_id {
            ModelID::Hash(hash) => Ok((
                serde_yaml::from_str(&fs::read_to_string(self.make_path(
                    &T::default(),
                    hash,
                    Self::SPEC_RELPATH,
                ))?)?,
                None,
                hash.to_owned(),
            )),
            ModelID::Annotation(name, version) => {
                let hash = self.lookup_hash(&T::default(), name, version)?;
                Ok((
                    serde_yaml::from_str(&fs::read_to_string(self.make_path(
                        &T::default(),
                        &hash,
                        Self::SPEC_RELPATH,
                    ))?)?,
                    serde_yaml::from_str(&fs::read_to_string(self.make_path(
                        &T::default(),
                        &hash,
                        Self::make_annotation_relpath(name, version),
                    ))?)?,
                    hash,
                ))
            }
        }
    }
    /// How to query any stored models.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue querying metadata from existing models in the store.
    pub(crate) fn list_model<T: Default + fmt::Debug>(&self) -> Result<Vec<ModelInfo>> {
        Ok(Self::find_model_metadata(&self.make_path(&T::default(), "**", "*"))?.collect())
    }
    /// How to explicitly delete any stored model and all associated annotations (does not propagate).
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an issue deleting a model from the store using `name` and
    /// `version`.
    pub(crate) fn delete_model<T: Default + fmt::Debug>(&self, model_id: &ModelID) -> Result<()> {
        // assumes propagate = false
        let hash = match model_id {
            ModelID::Hash(hash) => hash,
            ModelID::Annotation(name, version) => {
                &self.lookup_hash(&T::default(), name, version)?
            }
        };
        let spec_dir = self.make_path(&T::default(), hash, "");
        fs::remove_dir_all(spec_dir)?;

        Ok(())
    }
}
