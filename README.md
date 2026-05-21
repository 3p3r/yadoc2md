# yadoc2md

Yet Another Document to Markdown converter — a single Rust binary with CLI and REST modes.

## Build

```bash
cargo build --release
```

## Smoke tests

Run CLI and REST checks against every file in [`fixtures/`](fixtures/):

```bash
./smoke.sh
./coverage.sh
```

The script builds the binary, converts each supported fixture via `yadoc2md parse`, starts a temporary server, and exercises `POST /api/parse` plus health/OpenAPI/Swagger routes. Unsupported types (`sample.css`, `sample.mp4`, `sample.wav`) must fail in both modes.

The binary is `target/release/yadoc2md`.

## CI

GitHub Actions ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs on every push and pull request to `main`:

- `cargo test`, [`./coverage.sh`](coverage.sh) (90% line coverage on the library), and [`./smoke.sh`](smoke.sh)

On push to `main`, CI also cross-compiles release binaries for Linux (GNU and musl), macOS, and Windows using [`docker/Dockerfile.build`](docker/Dockerfile.build), then publishes them on the [**nightly**](https://github.com/3p3r/yadoc2md/releases/tag/nightly) pre-release:

| Artifact | Target |
|----------|--------|
| [yadoc2md-linux-x64](https://github.com/3p3r/yadoc2md/releases/download/nightly/yadoc2md-linux-x64) | `x86_64-unknown-linux-gnu` |
| [yadoc2md-linux-arm64](https://github.com/3p3r/yadoc2md/releases/download/nightly/yadoc2md-linux-arm64) | `aarch64-unknown-linux-gnu` |
| [yadoc2md-linux-musl-x64](https://github.com/3p3r/yadoc2md/releases/download/nightly/yadoc2md-linux-musl-x64) | `x86_64-unknown-linux-musl` |
| [yadoc2md-linux-musl-arm64](https://github.com/3p3r/yadoc2md/releases/download/nightly/yadoc2md-linux-musl-arm64) | `aarch64-unknown-linux-musl` |
| [yadoc2md-darwin-x64](https://github.com/3p3r/yadoc2md/releases/download/nightly/yadoc2md-darwin-x64) | `x86_64-apple-darwin` |
| [yadoc2md-darwin-arm64](https://github.com/3p3r/yadoc2md/releases/download/nightly/yadoc2md-darwin-arm64) | `aarch64-apple-darwin` |
| [yadoc2md-win32-x64.exe](https://github.com/3p3r/yadoc2md/releases/download/nightly/yadoc2md-win32-x64.exe) | `x86_64-pc-windows-gnu` |

## CLI

Convert a file to markdown (extension selects the backend):

```bash
# stdout
yadoc2md parse document.docx

# write to file
yadoc2md parse report.pdf -o report.md

# limits and strict mode (shared with serve)
yadoc2md parse --max-input-size 50MB --strict data.xlsx
```

### Shared conversion flags

| Flag | Default | Description |
|------|---------|-------------|
| `--max-input-size` | `100MB` | Maximum input file size |
| `--max-zip-size` | `500MB` | ZIP bomb guard for Office archives |
| `--max-image-bytes` | `50MB` | Cap on extracted image bytes |
| `--strict` | off | Fail on recoverable conversion warnings |
| `--pdf-password` | — | Password for encrypted PDFs |

## REST API

Start the server:

```bash
yadoc2md serve
yadoc2md serve --host 0.0.0.0 --port 9876 --cors http://localhost:3000
```

| Flag | Default | Description |
|------|---------|-------------|
| `--host` | `127.0.0.1` | Bind address |
| `--port` | `9876` | Bind port |
| `--cors` | `*` | Allowed origin (repeatable) |
| `--max-body-size` | same as `--max-input-size` | HTTP body limit |

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/` | Redirects to Swagger UI |
| `GET` | `/api/health` | Health check (`{"status":"ok"}`) |
| `POST` | `/api/parse` | Upload `multipart/form-data` field `file`, returns markdown |
| `GET` | `/api-doc/openapi.json` | OpenAPI 3 specification |
| `GET` | `/swagger-ui` | Swagger UI (points at the spec above) |

Example:

```bash
curl -s http://127.0.0.1:9876/api/health
curl -s -F "file=@document.pdf" http://127.0.0.1:9876/api/parse
# OpenAPI + Swagger UI (browser)
open http://127.0.0.1:9876/swagger-ui
```

Success: `200` with `Content-Type: text/markdown; charset=utf-8`.  
Errors: JSON `{"error":"..."}` with `400`, `413`, `415`, or `422` as appropriate.

## Supported formats

| Backend | Formats |
|---------|---------|
| [anytomd](https://github.com/developer0hye/anytomd-rs) | DOCX, PPTX, XLSX, XLS, HTML, CSV, IPYNB, JSON, XML, images, code, plain text, and more |
| [unpdf](https://github.com/iyulab/unpdf) | PDF |

PDF files are routed to **unpdf**; all other extensions use **anytomd** (which does not support PDF).

## Libraries

- CLI: [clap](https://github.com/clap-rs/clap)
- HTTP: [Salvo](https://salvo.rs/) ([OpenAPI](https://salvo.rs/guide/features/openapi.html) + Swagger UI)
- Conversion: [anytomd](https://github.com/developer0hye/anytomd-rs), [unpdf](https://github.com/iyulab/unpdf)

## License

MIT — see [LICENSE](LICENSE).
