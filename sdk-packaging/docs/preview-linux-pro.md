# Internal Linux Pro Preview

This is a private, recipient-bound, development/pre-production-only preview, not a GA release or public installation channel. Production and offline use require separate exceptions. The current public default remains v1.0.5. Distribution is not ready until the Manager binds the exact combined source candidate, qualified archive checksum, measured Linux distribution/glibc baseline, recipient grant, renewal owner, and support contact.

## Package and qualification identity

The existing Linux qualification run `36769666673` completed successfully from source `cb7c55b35696fe31900a79fc4ddd2b049ea222d0`. Its artifact is `nxuskit-sdk-pro-linux-x86_64`, containing `nxuskit-sdk-2.0.0-pro-linux-x86_64.tar.gz` and its checksum file. That archive is historical qualification evidence, not an archive of the newer preview integration candidate. It must not be repacked or relabeled.

The historical run verified native SDK/ABI version `2.0.0`, edition `pro`, release profile, target `x86_64-unknown-linux-gnu`, and installed CLI output `nxuskit-cli 2.0.0`. Extracted C, Go, Rust dynamic/static, and Python wheel/sdist consumers passed. The runner was `x360-Minty`; its measured distribution and glibc version were not captured in those historical receipts. Fresh no-publish qualification of the combined preview candidate is recorded below; distribution and activation gates remain open.

The package contains native libraries, CLI, C headers, Rust/Go/Python SDK inputs, examples, and package documentation. Preview Pro claims are Solver-only under the current capability gate. Compiled ZEN support is not a preview acceptance or availability promise.

The approved local qualification host was measured read-only on 2026-10-01: Linux Mint 22.3 (Zena), Ubuntu Noble lineage, x86_64, and glibc 2.39 (`Ubuntu GLIBC 2.39-0ubuntu8.9`). This is a measured qualification baseline, not a claim that every GNU/Linux distribution is supported. Run `36928672954` completed successfully from exact source `5e142106f47281f413f2417f8d7461086de3e4dc`, including extracted C, Go, Rust dynamic/static, Python wheel/sdist, CLI, native identity, and package-unchanged checks. Its `nxuskit-sdk-2.0.0-pro-linux-x86_64.tar.gz` is 100559404 bytes with SHA-256 `d21601277aba5c28219c3bcee89a420273e33173ef8bfd8aafc7635b80d28bb6`. This subsequent annotation does not relabel that run's source or archive; the archive does not contain the annotation itself.

## Retrieval after distribution approval

The cohort invitation must provide the approved private repository, exact qualified run, checksum, support contact, and supported host baseline. Do not substitute a public latest release or another workflow run. This source document does not authorize download or deployment.

Verify the provided archive checksum before extraction. Use the package's `bin/nxuskit-cli --version` to confirm the installed version, and use the packaged consumer instructions for the selected language. Native wrappers must resolve the libraries from this exact extracted package, not an ambient SDK installation. The fresh qualification receipt must record those wrapper paths and host architecture/glibc facts.

## Recipient-bound activation

The approved Licensing runbook defines a no-fee, finite-period, recipient-bound Pro entitlement using the existing commercial entitlement flow. It does not require Annual/Perpetual checkout and is not an `INT-` CI/QA grant. The grant owner must supply the purchase ID, machine cap, expiry/renewal policy, and support route before distribution.

The following commands are confirmed by current CLI source, but real installed activation has not yet been qualified for this preview. Execute them only after the separate grant and activation approval:

```bash
: "${NXUSKIT_SDK_DIR:?Set the verified extracted package directory}"
"$NXUSKIT_SDK_DIR/bin/nxuskit-cli" license login
"$NXUSKIT_SDK_DIR/bin/nxuskit-cli" license activate --key "$NXUSKIT_PREVIEW_PURCHASE_ID" --accept-eula --json
"$NXUSKIT_SDK_DIR/bin/nxuskit-cli" license status --json
"$NXUSKIT_SDK_DIR/bin/nxuskit-cli" license sync --json
```

Do not include tokens, purchase IDs, account/device identifiers, or private keys in support logs. Renewal uses the approved existing entitlement; there is no documented `license refresh` command. Do not invent a deactivation command. Fixture signing keys are not proof of customer activation. Production signing, offline licensing, and any conversion to GA entitlement remain separately governed.

## Publication isolation

The planned preview tag is `preview-v2.0.0-pro-linux.1`, with prerelease enabled and latest disabled. No preview tag or release has been created by this preparation. Both release-triggered publication workflows must contain the reviewed prerelease/draft/final-tag guards at the actual future tag target before distribution. A full-SHA Linux-only no-publish qualification is not publication or GA acceptance.
