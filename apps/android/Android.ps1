[CmdletBinding()]
param(
    [ValidateSet('Check', 'Init', 'Build', 'Dev')]
    [string]$Action = 'Check',
    [ValidateSet('x86_64', 'aarch64')]
    [string[]]$Target = @('x86_64', 'aarch64'),
    [string]$SdkPath = 'C:\Users\Marius\Desktop\android-tools\SDK',
    [string]$JavaPath = 'C:\Users\Marius\Desktop\android-tools\jdk-21.0.12.1+1',
    [string]$NdkVersion = '27.2.12479018',
    [switch]$TestSigning
)

$ErrorActionPreference = 'Stop'
$env:JAVA_HOME = (Resolve-Path -LiteralPath $JavaPath).Path
$env:ANDROID_HOME = (Resolve-Path -LiteralPath $SdkPath).Path
$env:NDK_HOME = Join-Path $env:ANDROID_HOME "ndk\$NdkVersion"
$env:PATH = "$env:JAVA_HOME\bin;C:\Program Files\nodejs;$env:ANDROID_HOME\platform-tools;$env:PATH"
foreach ($path in @("$env:JAVA_HOME\bin\java.exe", "$env:NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\bin\clang.exe", "$env:ANDROID_HOME\platforms\android-36\android.jar", "$env:ANDROID_HOME\build-tools\36.0.0\apksigner.bat")) {
    if (-not (Test-Path -LiteralPath $path)) { throw "Missing Android build prerequisite: $path" }
}

Push-Location $PSScriptRoot
try {
    switch ($Action) {
        'Check' {
            & java -version
            & cargo tauri --version
            & rustup target list --installed
            & adb devices
        }
        'Init' {
            if (Test-Path 'src-tauri/gen/android/app/build.gradle.kts') {
                throw 'Native Android project already exists. Preserve its configuration; do not reinitialize it.'
            }
            & cargo tauri android init --ci --skip-targets-install
        }
        'Build' {
            $dictionarySource = Join-Path $PSScriptRoot '../../jmdict-eng/jmdict-eng-3.6.2.json'
            if ((Get-FileHash -LiteralPath $dictionarySource -Algorithm SHA256).Hash.ToLowerInvariant() -ne '5f54504a62a7f45741e1bf6fd28f6e1e5add6405f2829748cc004a802839a3aa') { throw 'Unexpected dictionary source; update the version and provenance deliberately.' }
            & cargo run --offline --release --manifest-path ../../crates/dictionary-build/Cargo.toml -- ../../jmdict-eng/jmdict-eng-3.6.2.json src-tauri/assets/jmdict-20260928-v2.sqlite3
            if ($LASTEXITCODE -ne 0) { throw 'Dictionary provisioning failed.' }
            & "$PSScriptRoot/Generate-Notices.ps1"
            & npm.cmd run build
            if ($LASTEXITCODE -ne 0) { throw 'Android frontend build failed.' }
            $native = (Resolve-Path 'src-tauri/gen/android').Path
            $configuration = Get-Content 'src-tauri/tauri.conf.json' -Raw | ConvertFrom-Json
            $version = [Version]$configuration.version
            $versionCode = $version.Major * 1000000 + $version.Minor * 1000 + $version.Build
            if ($versionCode -lt 1 -or $versionCode -gt 2100000000) { throw 'Invalid Android version code.' }
            @("tauri.android.versionName=$($configuration.version)", "tauri.android.versionCode=$versionCode") |
                Set-Content -LiteralPath (Join-Path $native 'app/tauri.properties') -Encoding ascii
            $env:TAURI_ANDROID_PROJECT_PATH = $native
            $env:WRY_ANDROID_PACKAGE = 'com.tmw.companion'
            $env:WRY_ANDROID_LIBRARY = 'tmw_android_lib'
            $generated = Join-Path $native 'app/src/main/java/com/tmw/companion/generated'
            New-Item -ItemType Directory -Force -Path $generated | Out-Null
            $env:WRY_ANDROID_KOTLIN_FILES_OUT_DIR = $generated
            $llvm = Join-Path $env:NDK_HOME 'toolchains/llvm/prebuilt/windows-x86_64/bin'
            $tasks = @()
            $exclude = @()
            foreach ($abi in $Target) {
                if ($abi -eq 'aarch64') {
                    $triple = 'aarch64-linux-android'; $jni = 'arm64-v8a'; $flavor = 'Arm64'
                } else {
                    $triple = 'x86_64-linux-android'; $jni = 'x86_64'; $flavor = 'X86_64'
                }
                $linker = Join-Path $llvm "$($triple)24-clang.cmd"
                $upper = $triple.Replace('-', '_').ToUpperInvariant()
                $lower = $triple.Replace('-', '_')
                Set-Item -Path "Env:CARGO_TARGET_$($upper)_LINKER" -Value $linker
                Set-Item -Path "Env:CC_$lower" -Value $linker
                Set-Item -Path "Env:AR_$lower" -Value (Join-Path $llvm 'llvm-ar.exe')
                # NDK 27 needs explicit 16 KB ELF alignment for newer Android devices.
                & cargo rustc --manifest-path src-tauri/Cargo.toml --target $triple --release --lib --locked --features tauri/custom-protocol -- -C link-arg=-Wl,-z,max-page-size=16384 -C link-arg=-Wl,-z,common-page-size=16384
                if ($LASTEXITCODE -ne 0) { throw "Rust build failed for $triple." }
                $output = Join-Path $native "app/src/main/jniLibs/$jni"
                New-Item -ItemType Directory -Force -Path $output | Out-Null
                # Copy only app build output; this works without Windows symlink privileges.
                Copy-Item -LiteralPath "src-tauri/target/$triple/release/libtmw_android_lib.so" -Destination (Join-Path $output 'libtmw_android_lib.so') -Force
                $tasks += "assemble$($flavor)Release"
                $exclude += @('-x', "rustBuild$($flavor)Release")
            }
            $options = @('--no-daemon')
            if ($TestSigning) { $options += '-PtmwTestSigning=true' }
            & "$native/gradlew.bat" -p $native @tasks @exclude @options
        }
        'Dev' { & cargo tauri android dev }
    }
    if ($LASTEXITCODE -ne 0) { throw "Android $Action failed (exit $LASTEXITCODE)." }
} finally {
    Pop-Location
}



