$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    $raw = & cargo metadata --manifest-path src-tauri/Cargo.toml --filter-platform aarch64-linux-android --format-version 1 --locked --offline
    if ($LASTEXITCODE -ne 0) { throw 'Could not obtain locked dependency metadata.' }
    $metadata = $raw | ConvertFrom-Json
    $resolved = @{}
    foreach ($node in $metadata.resolve.nodes) { $resolved[$node.id] = $true }
    $text = [Text.StringBuilder]::new()
    $seenTexts = [Collections.Generic.HashSet[string]]::new()
    [void]$text.AppendLine("TMW Companion - Phase 2 dependency notices")
    [void]$text.AppendLine("JMdict Japanese-English data: copyright Electronic Dictionary Research and Development Group (EDRDG). jmdict-simplified 3.6.2, dictionary date 2026-09-28, CC BY-SA 4.0. Converted to indexed SQLite: English glosses retained, tags expanded, kanji/kana forms flattened with first kana reading, NFKC matching keys added. Data license is separate from application code. Source SHA256: 5f54504a62a7f45741e1bf6fd28f6e1e5add6405f2829748cc004a802839a3aa.")
    [void]$text.AppendLine("https://www.edrdg.org/edrdg/licence.html")
    [void]$text.AppendLine("https://github.com/scriptin/jmdict-simplified/blob/master/LICENSE.txt")
    [void]$text.AppendLine("Rust inventory includes build-time dependencies conservatively; not every listed package is linked into the APK.")
    $inventory = @('# Android Rust dependency license inventory', '', 'Generated from the locked ARM64 dependency graph. Includes build dependencies.', '', '| Package | Version | Declared license |', '| --- | --- | --- |')
    foreach ($package in ($metadata.packages | Sort-Object name, version)) {
        if (-not $resolved.ContainsKey($package.id) -or -not $package.source) { continue }
        if (-not $package.license) { throw "Missing declared license: $($package.name)" }
        $inventory += "| $($package.name) | $($package.version) | $($package.license) |"
        [void]$text.AppendLine("`n--- $($package.name) $($package.version) [$($package.license)] ---")
        [void]$text.AppendLine($package.repository)
        $directory = Split-Path $package.manifest_path
        foreach ($file in (Get-ChildItem -LiteralPath $directory -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)' })) {
            $licenseText = Get-Content -LiteralPath $file.FullName -Raw
            if ($seenTexts.Add($licenseText)) { [void]$text.AppendLine($licenseText) }
        }
    }
    foreach ($name in @('react', 'react-dom', '@tauri-apps/api', 'epubjs', 'jszip')) {
        $directory = Join-Path '../../node_modules' $name
        $package = Get-Content (Join-Path $directory 'package.json') -Raw | ConvertFrom-Json
        [void]$text.AppendLine("`n--- $name $($package.version) [$($package.license)] ---")
        foreach ($file in (Get-ChildItem -LiteralPath $directory -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|NOTICE)' })) {
            $licenseText = Get-Content -LiteralPath $file.FullName -Raw
            if ($seenTexts.Add($licenseText)) { [void]$text.AppendLine($licenseText) }
        }
    }
    [void]$text.AppendLine((Get-Content '../../docs/android/lindera-license.txt' -Raw))
    [void]$text.AppendLine((Get-Content '../../docs/android/jmdict-export-license.txt' -Raw))
    [void]$text.AppendLine("`n--- Yomitan Japanese language transforms [GPL-3.0-or-later] ---")
    [void]$text.AppendLine((Get-Content '../../crates/japanese-core/vendor/yomitan/NOTICE.txt' -Raw -Encoding UTF8))
    [void]$text.AppendLine((Get-Content '../../crates/japanese-core/vendor/yomitan/LICENSE' -Raw -Encoding UTF8))
    [IO.File]::WriteAllText((Join-Path $PSScriptRoot 'src/notices.txt'), $text.ToString())
    [IO.File]::WriteAllText((Join-Path $PSScriptRoot '../../docs/android/rust-licenses.txt'), ($inventory -join "`n") + "`n")
} finally { Pop-Location }

