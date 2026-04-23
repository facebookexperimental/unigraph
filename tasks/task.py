#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["typer>=0.15"]
# ///

import os
import shlex
import shutil
import subprocess
from pathlib import Path
from typing import Annotated

import typer

app = typer.Typer(help="Unigraph task runner.", invoke_without_command=True)
build_app = typer.Typer(help="Build artifacts.", invoke_without_command=True)
check_app = typer.Typer(
    help="Run linters and format checks.", invoke_without_command=True
)
fmt_app = typer.Typer(help="Run formatters.", invoke_without_command=True)
test_app = typer.Typer(help="Run tests.", invoke_without_command=True)

app.add_typer(build_app, name="build")
app.add_typer(check_app, name="check")
app.add_typer(fmt_app, name="fmt")
app.add_typer(test_app, name="test")

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "node_modules" / ".bin"


@app.callback()
def main(ctx: typer.Context) -> None:
    """Unigraph task runner."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@check_app.callback()
def check_main(ctx: typer.Context) -> None:
    """Run linters and format checks."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@build_app.callback()
def build_main(ctx: typer.Context) -> None:
    """Build artifacts."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@fmt_app.callback()
def fmt_main(ctx: typer.Context) -> None:
    """Run formatters."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


def run(args: list[str], env: dict[str, str] | None = None) -> None:
    cmd = Path(args[0])
    try:
        label = str(cmd.relative_to(ROOT))
    except ValueError:
        label = args[0]
    display = [label, *args[1:]]
    print(f"\033[90m$ {shlex.join(display)}\033[0m", flush=True)
    full_env = {**os.environ, **env} if env else None
    result = subprocess.run(args, cwd=ROOT, env=full_env)
    if result.returncode != 0:
        print(
            f"\n\033[1;31m"
            f"  ⢰⣶⣤⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢀\n"
            f"  ⠀⣿⣿⣿⣷⣤⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣤⣶⣾⣿\n"
            f"  ⠀⠘⢿⣿⣿⣿⣿⣦⣀⣀⣀⣄⣀⣀⣠⣀⣤⣶⣿⣿⣿⣿⣿⠇\n"
            f"  ⠀⠀⠈⠻⣿⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡿⠋⠀\n"
            f"  ⠀⠀⠀⠀⣰⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣟⠋⠀⠀⠀\n"
            f"  ⠀⠀⠀⢠⣿⣿⡏⠆⢹⣿⣿⣿⣿⣿⣿⠒⠈⣿⣿⣿⣇⠀⠀⠀\n"
            f"  ⠀⠀⠀⣼⣿⣿⣷⣶⣿⣿⣛⣻⣿⣿⣿⣶⣾⣿⣿⣿⣿⡀⠀⠀\n"
            f"  ⠀⠀⠀⡁⠀⠈⣿⣿⣿⣿⢟⣛⡻⣿⣿⣿⣟⠀⠀⠈⣿⡇⠀⠀\n"
            f"  ⠀⠀⠀⢿⣶⣿⣿⣿⣿⣿⡻⣿⡿⣿⣿⣿⣿⣶⣶⣾⣿⣿⠀⠀\n"
            f"  ⠀⠀⠀⠘⣿⣿⣿⣿⣿⣿⣿⣷⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡆⠀\n"
            f"  ⠀⠀⠀⠀⣼⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⡇⠀\n"
            f"\n"
            f"    Command failed with exit code {result.returncode}:\n"
            f"    $ {shlex.join(display)}\n"
            f"\033[0m",
            flush=True,
        )
        raise typer.Exit(result.returncode)


def fmt_size(size: int) -> str:
    kb = size / 1024
    return f"{kb:,.1f} kB"


def print_artifacts(paths: list[Path]) -> None:
    rows = []
    for p in paths:
        if p.exists():
            name = p.name
            size = fmt_size(p.stat().st_size)
            rows.append((name, size))
    if not rows:
        return
    name_w = max(len(r[0]) for r in rows)
    size_w = max(len(r[1]) for r in rows)
    sep = f"┼{'─' * (name_w + 2)}┼{'─' * (size_w + 2)}┼"
    print(f"\n{sep}")
    for name, size in rows:
        print(f"│ {name:<{name_w}} │ {size:>{size_w}} │")
    print(sep)


OUT_DIR = ROOT / ".build"
WASM_OUT_DIR = OUT_DIR / "wasm"
WASM_TARGET_DIR = Path("/tmp/unigraph-target")


def get_lockfile_version(package: str) -> str | None:
    """Read a package version from Cargo.lock."""
    lock = ROOT / "Cargo.lock"
    if not lock.exists():
        return None
    name_line = f'name = "{package}"'
    lines = lock.read_text().splitlines()
    for i, line in enumerate(lines):
        if line.strip() == name_line:
            next_line = lines[i + 1]
            if next_line.strip().startswith("version"):
                return next_line.split('"')[1]
    return None


def ensure_wasm_bindgen_cli() -> None:
    """Install wasm-bindgen-cli if missing or version-mismatched."""
    required = get_lockfile_version("wasm-bindgen")
    if required is None:
        return

    try:
        result = subprocess.run(
            ["wasm-bindgen", "--version"],
            capture_output=True,
            text=True,
        )
        installed = result.stdout.strip().removeprefix("wasm-bindgen ")
        if installed == required:
            return
        print(
            f"\033[33mwasm-bindgen-cli {installed} doesn't match "
            f"Cargo.lock ({required}), reinstalling…\033[0m"
        )
    except FileNotFoundError:
        print(f"\033[33mwasm-bindgen-cli not found, installing {required}…\033[0m")

    run(
        [
            "cargo",
            "install",
            "wasm-bindgen-cli",
            "--version",
            required,
            "--force",
        ]
    )


@build_app.command("clean")
def build_clean() -> None:
    """Remove all build artifacts under .build/."""
    if OUT_DIR.exists():
        shutil.rmtree(OUT_DIR, ignore_errors=True)
        print(f"Removed {OUT_DIR.relative_to(ROOT)}")


@build_app.command("wasm")
def build_wasm(
    skip_wasm_opt: Annotated[
        bool, typer.Option("--skip-wasm-opt", help="Skip wasm-opt optimization.")
    ] = False,
) -> None:
    """Build the WASM package."""
    WASM_OUT_DIR.mkdir(parents=True, exist_ok=True)
    ensure_wasm_bindgen_cli()

    run(
        [
            "cargo",
            "build",
            "--package",
            "unigraph_wasm",
            "--profile",
            "release-wasm",
            "--target",
            "wasm32-unknown-unknown",
        ]
    )

    wasm_path = (
        WASM_TARGET_DIR
        / "wasm32-unknown-unknown"
        / "release-wasm"
        / "unigraph_wasm.wasm"
    )
    run(
        [
            "wasm-bindgen",
            "--target",
            "web",
            str(wasm_path),
            "--out-dir",
            str(WASM_OUT_DIR),
        ]
    )

    wasm_artifact = WASM_OUT_DIR / "unigraph_wasm_bg.wasm"

    # Run wasm-opt for size reduction
    if skip_wasm_opt:
        pass
    elif not shutil.which("wasm-opt"):
        print(
            "\033[31mError: wasm-opt not found.\033[0m\n"
            "  Install: brew install binaryen\n"
            "  Or skip:  ut build wasm --skip-wasm-opt"
        )
        raise typer.Exit(1)
    else:
        run(
            [
                "wasm-opt",
                "-Oz",
                "--all-features",
                str(wasm_artifact),
                "-o",
                str(wasm_artifact),
            ]
        )

    print_artifacts(
        [
            wasm_artifact,
            WASM_OUT_DIR / "unigraph_wasm.js",
            WASM_OUT_DIR / "unigraph_wasm.d.ts",
        ]
    )


@build_app.command("tailwind")
def build_tailwind() -> None:
    """Build Tailwind CSS."""
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    run(
        [
            str(BIN / "tailwindcss"),
            "-i",
            "./input.css",
            "-o",
            "../.build/output.css",
            "--cwd",
            "u-fe",
        ]
    )


@build_app.command("js")
def build_js() -> None:
    """Build Tailwind CSS + JS bundle with type declarations."""
    build_wasm()
    build_tailwind()
    run([str(BIN / "rolldown"), "-c", "rolldown.config.ts"])

    # Copy the .wasm binary and CSS alongside the JS bundle
    js_out = OUT_DIR / "js"
    shutil.copy2(
        WASM_OUT_DIR / "unigraph_wasm_bg.wasm", js_out / "unigraph_wasm_bg.wasm"
    )
    shutil.copy2(OUT_DIR / "output.css", js_out / "Unigraph.css")


@build_app.command("all")
def build_all() -> None:
    """Build everything."""
    build_wasm()
    build_js()


@app.command()
def typegen() -> None:
    """Regenerate TypeScript/Flow/Hack types from Rust structs (runs tests with TYPEGEN=1)."""
    run(["cargo", "nextest", "run"], env={"TYPEGEN": "1"})


@app.command(
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True}
)
def serve(ctx: typer.Context) -> None:
    """Start Unigraph server. All flags are forwarded to `unigraph_cli serve`."""
    run(["cargo", "run", "-p", "unigraph_cli", "--", "serve", *ctx.args])


@app.command(
    "unigraph_turbopack",
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def unigraph_turbopack(ctx: typer.Context) -> None:
    """Convert Turbopack analyze data to Unigraph JSON. All args are forwarded."""
    run(["cargo", "run", "-p", "unigraph_turbopack_cli", "--", *ctx.args])


@app.command(
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True}
)
def cli(
    ctx: typer.Context,
    release: Annotated[
        bool, typer.Option("-r", "--release", help="Build in release mode.")
    ] = False,
) -> None:
    """Run unigraph_cli with arbitrary arguments. All args are forwarded."""
    cargo = ["cargo", "run", "-p", "unigraph_cli"]
    if release:
        cargo.append("--release")
    run([*cargo, "--", *ctx.args])


@app.command()
def dev() -> None:
    """Start dev server (Tailwind watch + Rolldown watch)."""
    tailwind = subprocess.Popen(
        [
            str(BIN / "tailwindcss"),
            "-i",
            "./input.css",
            "-o",
            "../.build/output.css",
            "--watch",
            "--cwd",
            "u-fe",
        ],
        cwd=ROOT,
    )
    rolldown = subprocess.Popen(
        [str(BIN / "rolldown"), "-c", "./rolldown.config.ts", "-w"],
        cwd=ROOT,
    )
    try:
        tailwind.wait()
        rolldown.wait()
    except KeyboardInterrupt:
        tailwind.terminate()
        rolldown.terminate()


@check_app.command("js")
def check_js() -> None:
    """Lint, type-check, and check formatting for JavaScript/TypeScript."""
    run([str(BIN / "oxlint"), "-c", ".oxlintrc.json", "u-fe/"])
    run([str(BIN / "oxfmt"), "--check", "u-fe/"])
    run([str(BIN / "tsc"), "--noEmit", "--project", "tsconfig.json"])


@check_app.command("rust")
def check_rust() -> None:
    """Lint and check formatting for Rust."""
    run(["cargo", "fmt", "--check"])
    run(["cargo", "clippy", "--all-targets"])


@check_app.command("all")
def check_all() -> None:
    """Run all checks."""
    check_js()
    check_rust()


@fmt_app.command("js")
def fmt_js() -> None:
    """Format JavaScript/TypeScript with oxfmt."""
    run([str(BIN / "oxfmt"), "u-fe/"])


@fmt_app.command("rust")
def fmt_rust() -> None:
    """Format Rust with cargo fmt."""
    run(["cargo", "fmt"])


@fmt_app.command("all")
def fmt_all() -> None:
    """Run all formatters."""
    fmt_js()
    fmt_rust()


@test_app.callback()
def test_main(ctx: typer.Context) -> None:
    """Run tests."""
    if ctx.invoked_subcommand is None:
        print(ctx.get_help())


@test_app.command("js")
def test_js() -> None:
    """Run JavaScript/TypeScript tests with vitest."""
    run([str(BIN / "vitest"), "run"])


@test_app.command(
    "rust",
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def test_rust(
    ctx: typer.Context,
    update: Annotated[
        bool, typer.Option("-u", "--update", help="Update k9 snapshots.")
    ] = False,
    filter: Annotated[
        str | None,
        typer.Argument(help="Filter by package/test name (substring match)."),
    ] = None,
) -> None:
    """Run Rust tests with cargo-nextest. Extra args are forwarded."""
    _run_rust_tests(update=update, filter=filter, extra_args=ctx.args)


def _run_rust_tests(
    update: bool = False,
    filter: str | None = None,
    extra_args: list[str] | None = None,
) -> None:
    env = {"K9_UPDATE_SNAPSHOTS": "1"} if update else None
    args = ["cargo", "nextest", "run"]
    if filter:
        args.extend(["-E", f"package(/{filter}/) + test(/{filter}/)"])
    if extra_args:
        args.extend(extra_args)
    run(args, env=env)


@test_app.command(
    "e2e",
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def test_e2e(ctx: typer.Context) -> None:
    """Run end-to-end browser tests with Playwright."""
    run([str(BIN / "playwright"), "test", *ctx.args])


@test_app.command("all")
def test_all(
    update: Annotated[
        bool, typer.Option("-u", "--update", help="Update k9 snapshots.")
    ] = False,
) -> None:
    """Run all tests."""
    test_js()
    _run_rust_tests(update=update)


@app.command()
def ci() -> None:
    """Run all checks and tests (for CI)."""
    check_all()
    test_all()
    run([str(BIN / "playwright"), "test"])


if __name__ == "__main__":
    app()
