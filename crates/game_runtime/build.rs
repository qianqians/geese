use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Link libpython when the python-runtime feature is active.
    // This is needed for the `--direct` loading mode (ctypes.CDLL) where
    // the DLL must resolve CPython API symbols itself rather than relying
    // on the host Python interpreter to provide them (extension-module mode).
    #[cfg(feature = "python-runtime")]
    {
        // 优先使用环境变量
        let py_lib = env::var("PYO3_PYTHON_LIB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| detect_python_lib());

        if py_lib.exists() {
            println!("cargo:rustc-link-search=native={}", py_lib.display());
        } else {
            panic!("Python library directory not found: {}", py_lib.display());
        }

        let lib_name = detect_python_lib_name();
        println!("cargo:rustc-link-lib={lib_name}");
    }
}

/// 通过查询 Python 解释器动态获取 libs 目录。
#[cfg(feature = "python-runtime")]
fn detect_python_lib() -> PathBuf {
    let output = Command::new("python")
        .args(["-c", "import sysconfig; print(sysconfig.get_config_var('LIBDIR'))"])
        .output()
        .expect("failed to run python");
    let libdir = String::from_utf8(output.stdout)
        .expect("invalid utf8")
        .trim()
        .to_string();
    PathBuf::from(libdir)
}

/// 检测 pythonXY 库名（如 python314）。
#[cfg(feature = "python-runtime")]
fn detect_python_lib_name() -> String {
    let output = Command::new("python")
        .args(["-c", "import sys; print(f'python{sys.version_info.major}{sys.version_info.minor}')"])
        .output()
        .expect("failed to run python");
    String::from_utf8(output.stdout)
        .expect("invalid utf8")
        .trim()
        .to_string()
}
