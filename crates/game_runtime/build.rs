use std::env;
use std::path::PathBuf;

fn main() {
    // Link libpython when the python-runtime feature is active.
    // This is needed for the `--direct` loading mode (ctypes.CDLL) where
    // the DLL must resolve CPython API symbols itself rather than relying
    // on the host Python interpreter to provide them (extension-module mode).
    #[cfg(feature = "python-runtime")]
    {
        let py_lib = env::var("PYO3_PYTHON_LIB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = env::var("USERPROFILE")
                    .or_else(|_| env::var("HOME"))
                    .expect("Cannot determine home directory");
                PathBuf::from(home)
                    .join("AppData")
                    .join("Local")
                    .join("Programs")
                    .join("Python")
                    .join("Python313")
                    .join("libs")
            });

        if py_lib.exists() {
            println!("cargo:rustc-link-search=native={}", py_lib.display());
        } else {
            panic!("Python library directory not found: {}", py_lib.display());
        }

        println!("cargo:rustc-link-lib=python313");
    }
}
