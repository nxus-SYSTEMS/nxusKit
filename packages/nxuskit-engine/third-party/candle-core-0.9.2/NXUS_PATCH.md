# Reviewed Candle ZIP parent patch

This is Candle Core 0.9.2 from the canonical crates.io archive
`c15b675b80d994b2eadb20a4bbe434eabeb454eac3ee5e2b4cf6f147ee9be091`,
published from upstream Git revision
`3b39794c14b1de8ba4c2d10d49354d557a415547` (`candle-core/`).

The original MIT OR Apache-2.0 declaration, LICENSE, README, Rust sources,
examples and upstream tests/fixtures are retained. The archive contains no native
C/C++/CUDA/Metal source. This does not grant any access to protected SDK CLIPS
or historical Z3 sources.

The manifest patch changes the ZIP dependency from 7.2.0 to 9.0.0, with default
features still disabled. The three filename consumers in `src/npy.rs` and
`src/pickle.rs` propagate ZIP's fallible filename results before processing the
name. This preserves eager/lazy NPZ names, including CP437 fallback, and PTH
enumeration without unwrapping or dropping errors. The two existing serialization
and PTH test files add exact public API value/name and malformed archive checks.
No other upstream Rust body or fixture changes. This does not spoof a ZIP version,
force an incompatible transitive override, remove a provider, or change a target
or Candle feature. Cargo.toml.upstream preserves the exact normalized original
manifest. NXUS_PATCH.json preserves all original file hashes, records the five
patched upstream paths and their current hashes, and records the explicit
omission of the historical upstream Cargo.lock; SDK workspace locks govern
the actual private/public resolutions.

The compatible published Mistral 0.7 parent requires Candle 0.9; newer reviewed
published parents still constrain ZIP below the advised fixed major. This
bounded local patch avoids a speculative unrelated inference/API migration.
It is not an assertion that no upstream fix can ever exist. Replace it with a
compatible published or reviewed immutable upstream parent once it admits
fixed ZIP, preserves required capabilities, and passes the NPZ/ZIP and Mistral
qualification gates.

The MSRV CI gate also runs the exact serialization (5 tests) and PTH (3 tests)
targets in a disposable Candle source copy. Its dependency preparation may not
introduce a version/source/checksum absent from the candidate lock. Both targets
must finish with zero failures/ignored/filtered tests; original source and lock
remain unchanged. Source-bound filename-body checks against ZIP 9.0.0 and 9.1.3
are narrower development evidence, not a claim that these full runtime targets
have executed. Local CPU metadata preparation stopped on an unavailable
other-platform `hermit-abi` index record before any Tensor/PTH build.

Both SDK workspace manifests explicitly patch this exact source path and exclude
it from workspace membership. No remote fork or publication is created.
The engine's feature-gated, synthetic ZIP consumer regressions exercise the real
Candle Tensor APIs through Mistral's public reexport, without loading a model or
calling a provider. Their successful execution and the affected Mistral feature
compile are required CI outputs, not implied by this provenance record.
