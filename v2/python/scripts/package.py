#!/usr/bin/env python3
"""Build, install, and smoke-test the V2 Python package."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path


V2_ROOT = Path(__file__).resolve().parents[2]
PYTHON_ROOT = V2_ROOT / "python"
DEFAULT_TARGET_DIR = Path("/tmp/restflow-v2-python-target")
DEFAULT_DIST_DIR = PYTHON_ROOT / "dist"


def build_env(python: str, target_dir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["PYO3_PYTHON"] = python
    env["CARGO_TARGET_DIR"] = str(target_dir)
    virtual_env = venv_root(Path(python))
    if virtual_env is not None:
        env["VIRTUAL_ENV"] = str(virtual_env)
    return env


def venv_root(python: Path) -> Path | None:
    candidate = python.absolute().parent.parent
    if (candidate / "pyvenv.cfg").exists():
        return candidate.resolve()
    return None


def maturin_command(action: str, release: bool, dist_dir: Path | None) -> list[str]:
    command = [sys.executable, "-m", "maturin", action]
    if release:
        command.append("--release")
    if action == "build" and dist_dir is not None:
        command.extend(["--out", str(dist_dir)])
    return command


def run(command: list[str], env: dict[str, str], cwd: Path = PYTHON_ROOT) -> None:
    subprocess.run(command, cwd=cwd, env=env, check=True)


def ensure_maturin(env: dict[str, str]) -> None:
    try:
        subprocess.run(
            [sys.executable, "-m", "maturin", "--version"],
            cwd=PYTHON_ROOT,
            env=env,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError as exc:
        raise SystemExit(
            "maturin is required. Install it with: python3 -m pip install maturin"
        ) from exc


def smoke(env: dict[str, str]) -> None:
    code = (
        "import json, stat, tempfile;"
        "from pathlib import Path;"
        "import restflow;"
        "client=restflow.CoreClient.native();"
        "response=client.handle(restflow.CoreCommand("
        "type='switch_model',"
        "model={'provider': {'id': 'openai'}, 'id': 'gpt-5.5'}"
        "));"
        "assert response.type == 'model_switched';"
        "assert response.payload['model']['id'] == 'gpt-5.5';"
        "import restflow.restflow_native;"
        "tmp=tempfile.TemporaryDirectory(prefix='restflow-v2-package-smoke-');"
        "root=Path(tmp.name)/'echo';"
        "(root/'bin').mkdir(parents=True);"
        "entry=root/'bin'/'echo';"
        "entry.write_text('#!/bin/sh\\ncat\\n');"
        "entry.chmod(entry.stat().st_mode | stat.S_IXUSR);"
        "(root/'artifact.json').write_text(json.dumps({"
        "'schema_version':1,"
        "'kind':'rust_binary',"
        "'id':'echo',"
        "'name':'Echo',"
        "'version':'0.1.0',"
        "'entry':'bin/echo'"
        "}));"
        "assert restflow.skill(root).call({'native': True}) == {'native': True};"
        "tmp.cleanup();"
        "print('restflow native smoke ok')"
    )
    run([sys.executable, "-c", code], env, cwd=Path("/tmp"))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        choices=["develop", "build", "smoke"],
        help="develop installs locally, build creates a wheel, smoke imports the installed package",
    )
    parser.add_argument(
        "--python",
        default=sys.executable,
        help="Python executable path used by PyO3",
    )
    parser.add_argument(
        "--target-dir",
        type=Path,
        default=DEFAULT_TARGET_DIR,
        help="Cargo target directory for native build artifacts",
    )
    parser.add_argument(
        "--dist-dir",
        type=Path,
        default=DEFAULT_DIST_DIR,
        help="Wheel output directory for build",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Use debug native artifacts instead of release artifacts",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    env = build_env(args.python, args.target_dir)

    if args.command == "smoke":
        smoke(env)
        return

    ensure_maturin(env)
    release = not args.debug
    dist_dir = args.dist_dir if args.command == "build" else None
    run(maturin_command(args.command, release, dist_dir), env)


if __name__ == "__main__":
    main()
