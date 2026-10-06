use std::fs;
use std::path::Path;

use crate::application::ports::{ProjectGenerationError, ProjectGeneratorPort};
use crate::domain::project::sanitize_target;

const TPL_DSP_H: &str = include_str!("templates/dsp_primitives.h.tpl");
const TPL_DSP_CPP: &str = include_str!("templates/dsp_primitives.cpp.tpl");
const TPL_EP_H: &str = include_str!("templates/effect_processor.h.tpl");
const TPL_EP_CPP: &str = include_str!("templates/effect_processor.cpp.tpl");
const TPL_HL_ADAPTER: &str = include_str!("templates/hl_adapter.cpp.tpl");
const TPL_HA_H: &str = include_str!("templates/hothouse_adapter.h.tpl");
const TPL_HA_CPP: &str = include_str!("templates/hothouse_adapter.cpp.tpl");
const TPL_MAIN: &str = include_str!("templates/main.cpp.tpl");
const TPL_MAKEFILE: &str = include_str!("templates/Makefile.tpl");
const TPL_GITIGNORE: &str = include_str!("templates/gitignore.tpl");

#[derive(Default)]
pub struct TemplateProjectGenerator;

impl TemplateProjectGenerator {
    pub fn new() -> Self {
        Self
    }
}

impl ProjectGeneratorPort for TemplateProjectGenerator {
    fn generate(&self, name: &str, path: &Path) -> Result<(), ProjectGenerationError> {
        let display_name = name.trim();
        let target = sanitize_target(display_name).ok_or_else(|| {
            ProjectGenerationError("Project name has no usable letters or digits.".into())
        })?;

        if path.is_file() {
            return Err(ProjectGenerationError(
                "That path is a file, not a folder.".into(),
            ));
        }
        if path.is_dir() && dir_non_empty(path) {
            return Err(ProjectGenerationError(
                "That folder already exists and isn't empty.".into(),
            ));
        }

        let created_dir = !path.exists();
        fs::create_dir_all(path).map_err(|e| {
            ProjectGenerationError(format!("Could not create the project folder: {e}"))
        })?;

        let main_file = format!("{target}.cpp");
        let files: [(&str, &str); 10] = [
            ("dsp_primitives.h", TPL_DSP_H),
            ("dsp_primitives.cpp", TPL_DSP_CPP),
            ("effect_processor.h", TPL_EP_H),
            ("effect_processor.cpp", TPL_EP_CPP),
            ("hl_adapter.cpp", TPL_HL_ADAPTER),
            ("hothouse_adapter.h", TPL_HA_H),
            ("hothouse_adapter.cpp", TPL_HA_CPP),
            (main_file.as_str(), TPL_MAIN),
            ("Makefile", TPL_MAKEFILE),
            (".gitignore", TPL_GITIGNORE),
        ];

        for (filename, template) in files {
            let contents = template
                .replace("{{TARGET}}", &target)
                .replace("{{PROJECT_NAME}}", display_name);
            if let Err(e) = fs::write(path.join(filename), contents) {
                if created_dir {
                    let _ = fs::remove_dir_all(path);
                }
                return Err(ProjectGenerationError(format!(
                    "Could not write {filename}: {e}"
                )));
            }
        }

        Ok(())
    }
}

fn dir_non_empty(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "hl_gen_test_{tag}_{}_{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        dir
    }

    #[test]
    fn generates_the_full_file_set_with_substitutions() {
        let dir = temp_dir("full");
        let generator = TemplateProjectGenerator::new();
        generator.generate("My Fuzz", &dir).unwrap();

        for f in [
            "dsp_primitives.h",
            "dsp_primitives.cpp",
            "effect_processor.h",
            "effect_processor.cpp",
            "hl_adapter.cpp",
            "hothouse_adapter.h",
            "hothouse_adapter.cpp",
            "my_fuzz.cpp",
            "Makefile",
            ".gitignore",
        ] {
            assert!(dir.join(f).exists(), "missing generated file: {f}");
        }

        let makefile = fs::read_to_string(dir.join("Makefile")).unwrap();
        assert!(makefile.contains("TARGET = my_fuzz"));
        assert!(!makefile.contains("{{TARGET}}"));

        let header = fs::read_to_string(dir.join("effect_processor.h")).unwrap();
        assert!(header.contains("My Fuzz"));
        assert!(!header.contains("{{PROJECT_NAME}}"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_non_empty_directory() {
        let dir = temp_dir("nonempty");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("existing.txt"), "hi").unwrap();

        let generator = TemplateProjectGenerator::new();
        assert!(generator.generate("Whatever", &dir).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_existing_empty_directory() {
        let dir = temp_dir("empty");
        fs::create_dir_all(&dir).unwrap();

        let generator = TemplateProjectGenerator::new();
        assert!(generator.generate("Empty Ok", &dir).is_ok());
        assert!(dir.join("empty_ok.cpp").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_unsanitizable_name() {
        let dir = temp_dir("badname");
        let generator = TemplateProjectGenerator::new();
        assert!(generator.generate("###", &dir).is_err());
        assert!(!dir.exists());
    }
}
