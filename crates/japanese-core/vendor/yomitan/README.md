# Pinned Yomitan Japanese transformations

Upstream commit: `77e200428902abf4fa48284df92da7af3dcb4162`.
Upstream: https://github.com/yomidevs/yomitan/tree/77e200428902abf4fa48284df92da7af3dcb4162
Copyright (C) 2024–2026 Yomitan Authors. GPL-3.0-or-later; see NOTICE.txt and LICENSE.

Original JS source is retained unchanged. `rules.json` is a generated literal-rule table
used by Rust; no JavaScript interpreter or network is needed in either app.
`language-transformer.js` is retained as the reference/oracle used by the comparison script.

Regenerate offline from repository root:

```powershell
& 'C:\Program Files\nodejs\node.exe' scripts/generate-yomitan-rules.mjs
& 'C:\Program Files\nodejs\node.exe' scripts/compare-yomitan-rules.mjs
```

The generator fails for nonliteral regex inputs rather than silently converting them.
All 889 current suffix/whole-word rules and 22 conditions are retained.
The Rust adapter preserves hierarchical input/output conditions and dictionary-form flags.
It caches indexed rule data once, deduplicates states, and applies TMW work limits.
Comparison cases record results from executing the original upstream transformer;
29 ordinary forms match, while expressive `信っじらんない` does not.
No custom expressive-spelling normalization is added.

Source and generated file SHA-256:

- `japanese-transforms.js`: `9e928202a2f8f8ed1a17961b3cd93b56cddaf9cdda98b8625dc689f8acdaf2a1`
- `language-transforms.js`: `e454f0a7333170aa1e96e42e546a316aee9c31b8b1d8056d45c034e1470c8a14`
- `language-transformer.js`: `f45db85ae6c5628b65ef0c45d48a47f6091f43109d723f81826dc4a1ce3ae198`
- `rules.json`: `96c83729ca53fe4e68458a23e97d16f1cdf9bf54343b126578cabd5f804e7b34`
- `comparison.json`: `64ed73555bbc8eb310100ee4ed260bf084b78d0a249341b9652c8c4faaa485c2`
- `LICENSE`: `8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903`
