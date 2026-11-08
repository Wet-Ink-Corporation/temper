use crate::PathStyle;

use super::{EnvExports, env_file_lines};

const NONE: EnvExports<'static> = EnvExports {
    cli: None,
    base: None,
    bin: None,
    bin_dir: None,
    style: PathStyle::Posix,
};

#[test]
fn the_1x_lines_come_first_byte_for_byte_and_d17_s_after() {
    let lines = env_file_lines(&EnvExports {
        cli: Some("/p/scripts/temper.mjs"),
        base: Some("1111"),
        bin: Some("/opt/t/temper"),
        bin_dir: Some("/opt/t"),
        ..NONE
    });

    assert_eq!(
        lines.split_inclusive('\n').collect::<Vec<_>>(),
        [
            "export TEMPER_CLI=\"/p/scripts/temper.mjs\"\n",
            "export TEMPER_BASE=\"1111\"\n",
            "export TEMPER_BIN=\"/opt/t/temper\"\n",
            "export PATH=\"/opt/t:$PATH\"\n",
        ],
        "1.x's two lines, then D17's (OC-6 A)"
    );
}

#[test]
fn a_missing_or_empty_base_and_a_missing_plugin_root_write_no_line() {
    assert_eq!(
        [
            env_file_lines(&EnvExports {
                cli: Some("/p/scripts/temper.mjs"),
                base: Some(""),
                ..NONE
            }),
            env_file_lines(&EnvExports {
                base: Some("abc"),
                ..NONE
            }),
            env_file_lines(&NONE),
        ],
        [
            "export TEMPER_CLI=\"/p/scripts/temper.mjs\"\n".to_owned(),
            "export TEMPER_BASE=\"abc\"\n".to_owned(),
            String::new(),
        ],
        "1.x's `baseRev(..) ? … : ''`; no TEMPER_CLI without a plugin root"
    );
}

#[test]
fn windows_paths_take_slashes_and_the_directory_its_msys_form() {
    let windows = |bin: &'static str, dir: &'static str| {
        env_file_lines(&EnvExports {
            bin: Some(bin),
            bin_dir: Some(dir),
            style: PathStyle::Windows,
            ..NONE
        })
    };

    assert_eq!(
        [
            windows(r"C:\Users\me\temper.exe", r"C:\Users\me"),
            windows(r"d:\temper.exe", r"d:\"),
            windows(r"\\srv\share\temper.exe", r"\\srv\share"),
        ],
        [
            "export TEMPER_BIN=\"C:/Users/me/temper.exe\"\nexport PATH=\"/c/Users/me:$PATH\"\n",
            "export TEMPER_BIN=\"d:/temper.exe\"\nexport PATH=\"/d/:$PATH\"\n",
            "export TEMPER_BIN=\"//srv/share/temper.exe\"\nexport PATH=\"//srv/share:$PATH\"\n",
        ],
        "Git Bash sources the file and splits PATH on `:`"
    );
}

#[test]
fn d17_values_are_escaped_for_double_quotes() {
    let lines = env_file_lines(&EnvExports {
        bin: Some("/a $b/\"c`/t"),
        bin_dir: Some("/a $b/\"c`"),
        ..NONE
    });

    assert_eq!(
        lines, "export TEMPER_BIN=\"/a \\$b/\\\"c\\`/t\"\nexport PATH=\"/a \\$b/\\\"c\\`:$PATH\"\n",
        "the shell reads the path back unchanged"
    );
}
