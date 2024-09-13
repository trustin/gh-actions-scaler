use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub struct TeardownPermissionDenied {
    path: PathBuf,
}

impl<P: AsRef<Path>> From<P> for TeardownPermissionDenied {
    fn from(path: P) -> Self {
        std::fs::set_permissions(path.as_ref(), std::fs::Permissions::from_mode(0o200))
            .expect("failed to set test file permissions (0o200)");
        TeardownPermissionDenied {
            path: PathBuf::from(path.as_ref()),
        }
    }
}

impl Drop for TeardownPermissionDenied {
    fn drop(&mut self) {
        std::fs::set_permissions(self.path.as_path(), std::fs::Permissions::from_mode(0o644))
            .expect("failed to set test file permissions (0o644)");
    }
}
