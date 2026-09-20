# Design diagrams

PlantUML sources for the crate's structure, and the rendered `.svg` beside each one.

The SVGs **are** committed (`git ls-files` lists them), so a diagram change should be
re-rendered in the same commit — otherwise the picture in the repo contradicts its source.

| File | What it shows |
|---|---|
| `01-component.puml` | the two crates, and why there are two |
| `02-pipeline.puml` | the three stages and what each is allowed to know |
| `03-pipeline-status.puml` | what is actually wired today, and where the gap is |
| `04-class-core.puml` | `Extraction` / `Extracted` / `Reason` and the trait contract |
| `05-class-resolution.puml` | the `Raw` → `Parsed` typestate |
| `06-class-meta.puml` | the `syn::Meta` translation layer and the shape bridge |
| `07-class-extractors.puml` | the concrete extractors and how they nest |
| `08-class-vocab.puml` | the vocabulary suite — declared names, generated conversions |

## Spinning up the renderer

The daemon is running but your user is not in the `docker` group, so one of these first:

```sh
# preferred — persists, but needs a re-login (or `newgrp docker`) to take effect
sudo usermod -aG docker "$USER" && newgrp docker

# or just prefix with sudo for now
```

Then:

```sh
docker run -d --rm -p 8080:8080 --name plantuml plantuml/plantuml-server:jetty
```

Give it a few seconds, then render everything:

The `charset=utf-8` is not optional — without it the server reads the files as latin-1 and every
em-dash renders as `â€"`.

```sh
cd docs/design
for f in *.puml; do
  curl -sf --data-binary @"$f" -X POST \
    -H 'Content-Type: text/plain; charset=utf-8' \
    http://localhost:8080/svg -o "${f%.puml}.svg" \
    && echo "ok   $f" || echo "FAIL $f"
done
```

`http://localhost:8080` also serves an interactive editor — paste a file in to iterate on layout.

When done: `docker stop plantuml`.

Without a container, if `plantuml` is installed locally: `plantuml -tsvg docs/design/*.puml`.

## Reading order

For a walkthrough, in this order:

1. **`01`** — why there are two crates. One rustc constraint explains the whole layout.
2. **`02`** — the rule everything follows from: *extraction carries, processing understands*.
3. **`03`** — the honest status. Read this before believing any of the others describe running code.
4. **`08`** — the vocabulary suite, which is where most of the recent work went.
5. **`04`**, `06`, `05`, `07` — reference, in roughly decreasing order of how much they decide.

## A warning about these files

Notes quote the `#id` annotations in the source. Nothing keeps them in sync, and they are written by
hand. **Where a diagram and the code disagree, the code is right and the diagram is stale.**
