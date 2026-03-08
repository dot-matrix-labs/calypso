# Documentation Standard

**"The codebase is the map."** Every directory describes itself. No RAG, no vector DB needed.

## Rules
1. **Root anchor:** `README.md` at project root points to `./docs/` and `./src/`.
2. **No stranded docs:** no `.md`/`.txt`/`.rst` outside `./docs/` (except `README.md` per directory).
3. **Fractal:** every subdirectory has `README.md` — purpose, key files, link to parent docs.
4. **Naming:** all docs in `./docs/` use kebab-case.

## Enforcement (pre-push hook)
```bash
#!/bin/sh
while read local_ref local_sha remote_ref remote_sha; do
    if [ "$remote_ref" = "refs/heads/"* ]; then
        [ "$local_sha" = "0000000000000000000000000000000000000000" ] && range="$remote_sha" || range="$remote_sha..$local_sha"
        invalid=$(git diff --name-only $range 2>/dev/null | grep -E '\.(md|txt|rst)$' | grep -v '^docs/' | grep -v '/README.md$' | grep -v '^README.md$' || true)
        [ -n "$invalid" ] && echo "ERROR: docs outside ./docs/: $invalid" && exit 1
    fi
done
exit 0
```
