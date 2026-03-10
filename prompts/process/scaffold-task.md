# Calypso Scaffold Task (Agent Entrypoint)

**Role:** You are an autonomous agent bootstrapping a new Calypso project from scratch. Handle all steps autonomously. Only pause for human input when credentials or decisions are genuinely required.

---

## Step 1: Provision Linux Host

- [ ] **Linux host is available.** If the current machine is Linux, proceed. If not, provision a remote Linux host (Ubuntu 22.04+ recommended) using whatever cloud provider API or credentials you have access to (DigitalOcean, AWS, GCP, Hetzner, etc.).
- [ ] All subsequent steps execute on that host — SSH in and run commands there.

---

## Step 2: Set Up Kubernetes Cluster

Install a single-node Kubernetes cluster on the host and deploy the Calypso dev environment using the published base images.

- [ ] k3s installed and `kubectl` configured.
- [ ] **Collect the user's SSH public key** before deploying — ask the user to paste their public key (e.g. `~/.ssh/id_ed25519.pub`). Create the Kubernetes secret the dev pod depends on: `kubectl create secret generic dev-ssh-keys --from-literal=authorized_keys="<public-key>" -n calypso`
- [ ] Apply all Calypso `k8s/` manifests. These reference the published upstream images (`ghcr.io/dot-matrix-labs/calypso/*`) and provide a running dev environment immediately, before the project's own CI pipeline exists.
- [ ] All four containers running and healthy: `dev`, `frontend`, `worker`, `db`.
- [ ] Verify the frontend serves HTTP 200 at `http://<host-ip>:<frontend-nodeport>/health`.

---

## Step 3: Hand Off SSH to User

- [ ] Output SSH connection details: host/IP, port, username, and key path if applicable.
- [ ] Instruct the user to connect via terminal (`ssh user@host`) or an IDE with SSH remote support (VS Code Remote-SSH, JetBrains Gateway, etc.) for file editing.
- [ ] Wait for the user to confirm they are connected before continuing.

---

## Step 4: Create Project Repo

Working inside the dev container on the remote host:

- [ ] Shallow-clone the Calypso repo: `git clone --depth 1 https://github.com/dot-matrix-labs/calypso.git <project-name>`
- [ ] Create a new private repo on the user's GitHub account using `gh repo create` (the dev container is assumed to be configured with the user's `gh` credentials).
- [ ] Set the new repo as origin and push: `git remote set-url origin <new-repo-url> && git push -u origin main`
- [ ] Extract the kubeconfig, replacing the loopback address with the host's public IP so GitHub Actions can reach the cluster, and set it as a repo secret:
  ```bash
  PUBLIC_IP=$(curl -s ifconfig.me)
  kubectl config view --raw \
    | sed "s|https://127.0.0.1:6443|https://${PUBLIC_IP}:6443|g" \
    | base64 -w0 \
    | gh secret set KUBE_CONFIG -R <new-repo> --body -
  ```

---

## Step 5: Read All Prompts

Read every file in the `prompts/` directory of the cloned repo before proceeding. These define the architecture, process, security posture, UX standards, and implementation conventions for the entire project.

```
prompts/blueprints/
prompts/development/
prompts/implementation-ts/
prompts/process/
```

---

## Step 6: Product Owner Interview

Conduct the product owner interview per `prompts/process/product-owner-interview.md`. Do not skip or abbreviate it.

- [ ] Interview completed.
- [ ] Canonical PRD written to `docs/prd.md`.
- [ ] External API test credentials collected and stored in `.env.test` (not committed).

---

## Step 7: Scaffold

Verify all foundational elements are present before moving to prototyping. Fix anything missing yourself before proceeding.

### Architecture
- [ ] Monorepo structure established (`/apps/web`, `/apps/server`, `/packages/*`).
- [ ] All modules use TypeScript, Bun, React, and Tailwind CSS exclusively.
- [ ] `package.json` scripts use `bunx`, not globally installed binaries.
- [ ] Local monorepo dependencies are built in correct order before dependent modules.
- [ ] All modules pass `bun run build`.
- [ ] Linting and formatting are configured and pass cleanly.

### Documentation
- [ ] `docs/` directory exists at repo root. No docs outside it except per-directory `README.md` files.
- [ ] Code comments on every source file: module purpose, key types, and function definitions.
- [ ] `.git/hooks/pre-push` installed per `prompts/process/documentation-standard.md`.

### Testing
- [ ] Vitest and Playwright configured.
- [ ] No mocking libraries present (`jest.mock`, `msw`, etc.).
- [ ] Stub test files exist for all categories: server (unit, module, integration) and browser (unit, component, e2e).
- [ ] Full test suite passes.
- [ ] No lint or format warnings.
- [ ] E2e test starts the production `bun` server, which serves a stub HTML page, and Playwright drives it successfully.
- [ ] Playwright HTML reporter set to `open: 'never'`.

### Deployment
- [ ] `.env` template files present.
- [ ] `k8s/` manifests updated to reference the project's own registry (`ghcr.io/<your-org>/<your-project>/*`). Once CI runs, it replaces the upstream base images via `kubectl set image`.
- [ ] GitHub Actions workflows adapted from `.github/workflows/` in the Calypso repo: build images on push to `main`, push to `ghcr.io/<your-org>/<your-project>`, deploy to cluster via `kubectl set image`.
- [ ] `KUBE_CONFIG` secret confirmed set (done in Step 4). `GITHUB_TOKEN` is provided automatically by GitHub Actions.
- [ ] A test CI run completes successfully: image built, pushed to registry, and deployed to the cluster.

### Final Check
- [ ] **No JS configs** — delete any `.js`/`.mjs` config files generated by scaffolding tools and rewrite in `.ts`.
- [ ] **No emit** — `"noEmit": true` in `tsconfig.json` for all `/packages/*`.
- [ ] **SPA routing** — Bun server checks `Bun.file.exists()` before falling back to `index.html`.
- [ ] **Blueprint compliance** — review all blueprint documents and confirm nothing was violated or skipped.

---

**Action Required:**
If all items above are verified, output:
`[VERIFIED] Scaffold successful. Awaiting command to begin Prototype phase.`

If anything fails, output the failing items, plan the fixes, and execute them immediately.
