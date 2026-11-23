#!/usr/bin/env python3
"""Create a Linux-runnable copy of the vali3 PHP API.

The upstream PHP implementation targets Windows: it uses backslash path
separators and executes the validator executables directly. In CI both
implementations run in the same wine based image, so this script makes a copy
of vali3/ that can be served by PHP's built-in web server on Linux. Only paths,
PHP 8 compatibility and the wine hand-off are adjusted; the validation logic
itself is left untouched.
"""

from __future__ import annotations

import argparse
import os
import shutil
import stat
import sys
from pathlib import Path

IVSHELL_WRAPPER = """#!/bin/sh
# Wine needs Windows paths, so convert absolute paths that exist on this
# filesystem to their Z:\\... equivalent before handing them to ivshell.exe.
set -eu

dir=$(dirname -- "$0")
quoted_args=""
for arg in "$@"; do
    case "$arg" in
        /*)
            if [ -e "$arg" ]; then
                arg="Z:$(printf '%s' "$arg" | sed 's|/|\\\\|g')"
            fi
            ;;
    esac

    escaped=$(printf '%s' "$arg" | sed "s/'/'\\\\''/g")
    quoted_args="$quoted_args '$escaped'"
done

eval "exec wine \\"$dir/ivshell.real.exe\\" $quoted_args"
"""


def replace_once(text: str, old: str, new: str, path: Path) -> str:
    if text.count(old) != 1:
        raise SystemExit(
            f"{path}: expected exactly one occurrence of {old!r}, "
            f"found {text.count(old)}. The vali3 submodule may have changed."
        )
    return text.replace(old, new, 1)


def patch_php_api(path: Path) -> None:
    text = path.read_text()

    text = replace_once(
        text,
        "define('VALI_DIR_CONF', 'c:/wwwroot/vali');",
        "define('VALI_DIR_CONF', '/app/vali3');",
        path,
    )

    # Use POSIX separators instead of Windows backslashes.
    text = replace_once(
        text,
        "    # Replaces forward slashes with backslashes\n    \n"
        "    return str_replace('/', '\\\\', $value);",
        "    # Keep POSIX separators, this copy runs on Linux.\n    \n"
        "    return $value;",
        path,
    )
    text = replace_once(
        text,
        "    if (substr($path, strlen($path) - 1, 1) !== '\\\\')\n"
        "    {\n"
        "      $path .= '\\\\';\n"
        "    }",
        "    if (substr($path, strlen($path) - 1, 1) !== '/')\n"
        "    {\n"
        "      $path .= '/';\n"
        "    }",
        path,
    )

    # get_magic_quotes_gpc() was removed in PHP 8.
    text = replace_once(
        text,
        "if (get_magic_quotes_gpc())",
        "if (function_exists('get_magic_quotes_gpc') && get_magic_quotes_gpc())",
        path,
    )

    path.write_text(text)


def patch_php_engine(common_path: Path, fx_path: Path) -> None:
    # fn is a reserved word since PHP 7.4 (arrow functions), so the class cannot
    # be parsed by the PHP version in the Debian image.
    text = common_path.read_text()
    text = replace_once(text, "\nclass fn\n", "\nclass xfn\n", common_path)
    text = text.replace("fn::", "xfn::")
    common_path.write_text(text)

    # Notices and deprecations did not exist / were not fatal on the PHP version
    # this code was written for. Keep the error handler focused on real errors.
    text = fx_path.read_text()
    text = replace_once(
        text,
        "error_reporting(E_ALL);",
        "error_reporting(E_ALL & ~E_DEPRECATED & ~E_NOTICE);",
        fx_path,
    )
    text = replace_once(
        text,
        "public static function handlerException(Exception $Exception)",
        "public static function handlerException(Throwable $Exception)",
        fx_path,
    )
    text = replace_once(
        text,
        "private static function logException(Exception $Exception, $inException = false)",
        "private static function logException(Throwable $Exception, $inException = false)",
        fx_path,
    )
    fx_path.write_text(text)


def patch_config(path: Path, cgi_dir: Path) -> None:
    # Windows file names are case insensitive, Linux is not. Resolve module
    # names the same way the Rust implementation does so file_exists() succeeds.
    module_files: dict[str, str] = {}
    for entry in cgi_dir.iterdir():
        if entry.is_file():
            module_files.setdefault(entry.name.lower(), entry.name)

    lines = []
    for line in path.read_text().splitlines(keepends=True):
        if line.startswith("LogDir "):
            line = "LogDir /tmp\n"
        elif line.startswith("ModuleDir "):
            line = "ModuleDir /app/vali3/cgi-bin\n"
        elif line.startswith("Module "):
            parts = line.split()
            if len(parts) >= 3:
                actual = module_files.get(parts[2].lower())
                if actual:
                    parts[2] = actual
                    line = " ".join(parts) + "\n"
        lines.append(line)
    path.write_text("".join(lines))


def install_ivshell_wrapper(path: Path) -> None:
    real = path.with_name("ivshell.real.exe")
    if not real.exists():
        path.rename(real)

    path.write_text(IVSHELL_WRAPPER)
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", default="vali3", type=Path)
    parser.add_argument("--destination", required=True, type=Path)
    args = parser.parse_args()

    source = args.source.resolve()
    destination = args.destination.resolve()

    if not (source / "app/api/vali-igc.php").is_file():
        raise SystemExit(f"{source} does not look like the vali3 source tree")

    if destination.exists():
        shutil.rmtree(destination)

    # Only the files needed to serve the API are copied. The samples and the
    # web front end are not used by the PHP server.
    destination.mkdir(parents=True)
    for entry in ("app", "cgi-bin", "api.php", "index.php", "vali-igc.conf"):
        src = source / entry
        dst = destination / entry
        if src.is_dir():
            shutil.copytree(src, dst, ignore=shutil.ignore_patterns(".git"))
        else:
            shutil.copy2(src, dst)

    patch_php_api(destination / "app/api/vali-igc.php")
    patch_php_engine(
        destination / "app/engine/common.php",
        destination / "app/system/fx.php",
    )
    patch_config(destination / "vali-igc.conf", destination / "cgi-bin")
    install_ivshell_wrapper(destination / "app/api/ivshell.exe")

    os.chmod(destination, 0o755)
    print(f"Prepared PHP server tree in {destination}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
