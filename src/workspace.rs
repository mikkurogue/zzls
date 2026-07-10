use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

#[allow(dead_code)]
impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Find build.zig in the workspace
    pub fn find_build_file(&self) -> Option<PathBuf> {
        let build_zig = self.root.join("build.zig");
        if build_zig.exists() {
            Some(build_zig)
        } else {
            None
        }
    }

    /// Find build.zig.zon in the workspace
    pub fn find_build_zon(&self) -> Option<PathBuf> {
        let build_zon = self.root.join("build.zig.zon");
        if build_zon.exists() {
            Some(build_zon)
        } else {
            None
        }
    }

    /// Find all .zig files in the workspace
    pub fn find_zig_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        self.find_zig_files_recursive(&self.root, &mut files);
        files
    }

    fn find_zig_files_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip common directories that shouldn't be indexed
                    let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
                    if !["zig-cache", "zig-out", ".git", "node_modules", ".zig-cache"].contains(&dir_name.as_ref()) {
                        self.find_zig_files_recursive(&path, files);
                    }
                } else if path.extension().map_or(false, |ext| ext == "zig" || ext == "zon") {
                    files.push(path);
                }
            }
        }
    }

    /// Find the main entry point of the project
    pub fn find_main_file(&self) -> Option<PathBuf> {
        // Check for src/main.zig
        let src_main = self.root.join("src").join("main.zig");
        if src_main.exists() {
            return Some(src_main);
        }

        // Check for main.zig in root
        let root_main = self.root.join("main.zig");
        if root_main.exists() {
            return Some(root_main);
        }

        // Check for src/lib.zig
        let src_lib = self.root.join("src").join("lib.zig");
        if src_lib.exists() {
            return Some(src_lib);
        }

        None
    }

    /// Check if this is a valid Zig project
    pub fn is_valid_project(&self) -> bool {
        self.find_build_file().is_some() || self.find_main_file().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_workspace_new() {
        let root = PathBuf::from("/tmp/test");
        let workspace = Workspace::new(root.clone());
        assert_eq!(workspace.root(), &root);
    }

    #[test]
    fn test_find_build_file() {
        let temp_dir = std::env::temp_dir().join("zzls_test_workspace");
        fs::create_dir_all(&temp_dir).unwrap();
        fs::write(temp_dir.join("build.zig"), "").unwrap();

        let workspace = Workspace::new(temp_dir.clone());
        assert!(workspace.find_build_file().is_some());

        fs::remove_dir_all(temp_dir).unwrap();
    }
}
