# GitBook Deployment

This is the maintainer runbook for publishing the repository documentation with
GitBook. GitBook hosts the site and synchronizes Markdown from GitHub; LumAgg
does not need a documentation server or a GitHub Actions deployment workflow.

## Repository Layout

GitBook configuration is already present at the repository root:

```text
.gitbook.yaml
docs/
  README.md
  SUMMARY.md
  ...
```

`.gitbook.yaml` points GitBook at the documentation directory:

```yaml
root: ./docs/

structure:
  readme: README.md
  summary: SUMMARY.md
```

`docs/README.md` is the home page and `docs/SUMMARY.md` controls the public
navigation. Grant evidence and internal working documents that are omitted from
`SUMMARY.md` do not appear in the main navigation.

## 1. Push the Documentation

GitBook can only import committed GitHub content. Before connecting it, commit
and push `.gitbook.yaml`, the public Markdown files, and referenced assets to
the `main` branch of:

```text
Lum-Agg/stellar-dex-agg
```

Confirm the files are visible on GitHub before starting the initial sync.

## 2. Create the GitBook Content Space

1. Sign in to GitBook and create or open the LumAgg organization.
2. Create a docs site named `LumAgg Documentation`.
3. Choose the option to sync content from GitHub, or create an empty space and
   select `Set up Git Sync` from that space.
4. Install the GitBook GitHub App for the `Lum-Agg` organization.
5. Grant the app access only to `stellar-dex-agg` unless other repositories are
   intentionally going to use GitBook.

Only a GitBook organization administrator or creator can configure Git Sync.

## 3. Configure Git Sync

Use these exact values:

| Setting | Value |
| --- | --- |
| Provider | GitHub |
| Repository | `Lum-Agg/stellar-dex-agg` |
| Branch | `main` |
| Project directory | Empty or `/` |
| Initial sync direction | GitHub to GitBook |

Do not set the Project directory to `docs/`. GitBook first searches the Project
directory for `.gitbook.yaml`; that file is at the repository root. The
`root: ./docs/` setting inside the YAML then selects the content directory.

For the first synchronization, choose **GitHub to GitBook**. Choosing the
opposite direction risks exporting an empty or template GitBook space over the
repository documentation.

After import, verify:

- The first page is `LumAgg Documentation` from `docs/README.md`.
- The sidebar order matches `docs/SUMMARY.md`.
- Production Aggregator, Swap API, Arbitrage, contracts, and API pages open.
- Internal SCF drafts are not in the main navigation.
- Local links and code blocks render correctly.

## 4. Publish the Site

GitBook separates editable content spaces from published docs sites. Open the
`LumAgg Documentation` docs site, link the synchronized space if it is not
already linked, set the audience to `Public`, and click `Publish`.

The current public GitBook URL is:

```text
https://lumagg.gitbook.io/
```

GitBook may also provide a site URL similar to:

```text
https://<organization>.gitbook.io/<site-slug>
```

Use this URL for the initial review before configuring DNS.

## 5. Configure `docs.lumagg.xyz`

Custom domains are currently available on GitBook Premium and Ultimate site
plans. If the site uses one of those plans:

1. Open the docs site dashboard.
2. Go to `Settings` -> `Domain and URL`.
3. Select custom domain and enter `docs.lumagg.xyz`.
4. Copy the exact DNS record type, name, and target shown by GitBook.
5. Add that record at the DNS provider for `lumagg.xyz`.
6. Wait for DNS propagation, return to GitBook, and complete verification.

Do not hardcode a CNAME target from another GitBook site; use the value GitBook
shows for this site. GitBook provisions the TLS certificate automatically after
DNS validation.

For Cloudflare-managed DNS, keep the GitBook CNAME in **DNS only** mode while
the domain and certificate are being verified. A proxied record can prevent
GitBook from validating the hostname.

Useful checks:

```bash
dig CNAME docs.lumagg.xyz
curl -I https://docs.lumagg.xyz
```

If a paid GitBook plan is not desired yet, publish with the provided
`gitbook.io` URL. A custom-domain static documentation stack such as Docusaurus
or MkDocs can be evaluated separately; it should not be introduced in parallel
with GitBook unless the hosting decision changes.

## Update Workflow

Once Git Sync is active, the normal workflow is:

```text
edit docs -> pull request -> merge to main -> GitBook sync -> live site update
```

No release tag or manual GitBook deployment is required for documentation
updates. The primary GitHub branch is the source of truth. Avoid editing
`README.md`, `SUMMARY.md`, or `.gitbook.yaml` in the GitBook editor because the
integration is bidirectional and UI edits can create repository commits or
conflicts.

Before merging a documentation change:

- Check that every new public page is intentionally added to `docs/SUMMARY.md`.
- Keep maintainer, grant, and evidence files out of the public navigation.
- Use relative links for repository Markdown and HTTPS links for external
  resources.
- Preview desktop and mobile layouts in GitBook.
- Confirm that API examples contain no credentials, private RPC endpoints, or
  internal deployment paths.

## Recommended Site Settings

- Site title: `LumAgg Documentation`
- Description: `Stellar liquidity aggregation, Swap API, and arbitrage operator documentation.`
- Audience: Public and indexed by search engines
- Default content: LumAgg Documentation space
- Logo and favicon: LumAgg production brand assets
- Header links: `LumAgg`, `GitHub`, and `API Status`
- Primary URL after verification: `https://docs.lumagg.xyz`

The initial release should use one space. Separate API-reference, version, or
language spaces can be added later only when the content is large enough to
justify GitBook site sections or variants.
