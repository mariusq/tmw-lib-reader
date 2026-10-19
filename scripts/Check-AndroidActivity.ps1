[CmdletBinding()]
param([string]$Serial)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot -Parent
$native=Join-Path $repo 'apps/android/src-tauri/gen/android'
$fixture=Join-Path $repo '.test-results/android-activity'
$env:JAVA_HOME='C:\Users\Marius\Desktop\android-tools\jdk-21.0.12.1+1'
$env:ANDROID_HOME='C:\Users\Marius\Desktop\android-tools\SDK'
$adb=Join-Path $env:ANDROID_HOME 'platform-tools/adb.exe'
if(-not $Serial) {
    $Serial=(& $adb devices | Select-String '^emulator-\d+\s+device$' | Select-Object -First 1).ToString().Split("`t")[0]
}
if(-not $Serial -or $Serial -notmatch '^emulator-\d+$') {throw 'Specify the running TMW_Test emulator serial; physical devices are excluded.'}
if((& $adb -s $Serial emu avd name | Select-Object -First 1) -ne 'TMW_Test') {throw 'Use the existing TMW_Test emulator.'}
$env:ANDROID_SERIAL=$Serial
New-Item -ItemType Directory -Force -Path "$fixture/app/src/main/java/com/tmw/companion","$fixture/app/src/androidTest/java/com/tmw/companion" | Out-Null
@'
pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
include ':app'
'@ | Set-Content -LiteralPath "$fixture/settings.gradle" -Encoding utf8
@'
buildscript {
    repositories { google(); mavenCentral() }
    dependencies { classpath("com.android.tools.build:gradle:9.3.1"); classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.2.10") }
}
allprojects { repositories { google(); mavenCentral() } }
'@ | Set-Content -LiteralPath "$fixture/build.gradle.kts" -Encoding utf8
@'
plugins { id("com.android.application"); id("org.jetbrains.kotlin.android") }
android {
    namespace = "com.tmw.companion"
    compileSdk = 36
    defaultConfig {
        applicationId = "com.tmw.activityfixture"
        minSdk = 24
        targetSdk = 36
        versionName = "fixture"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }
    buildFeatures { buildConfig = true }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_1_8; targetCompatibility = JavaVersion.VERSION_1_8 }
}
kotlin { compilerOptions { jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8 } }
dependencies {
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test:runner:1.5.0")
    androidTestImplementation("androidx.lifecycle:lifecycle-common:2.10.0")
    androidTestImplementation("androidx.annotation:annotation-experimental:1.4.1")
}
'@ | Set-Content -LiteralPath "$fixture/app/build.gradle.kts" -Encoding utf8
@'
android.builtInKotlin=false
android.newDsl=false
android.useAndroidX=true
org.gradle.jvmargs=-Xmx2048m -Dfile.encoding=UTF-8
'@ | Set-Content -LiteralPath "$fixture/gradle.properties" -Encoding utf8
'<manifest xmlns:android="http://schemas.android.com/apk/res/android"><application android:label="Isolated tracker tests"/></manifest>' | Set-Content -LiteralPath "$fixture/app/src/main/AndroidManifest.xml" -Encoding utf8
'package com.tmw.companion; internal object MainActivity { var readingForeground=true; fun activeIntervals(start:Long,end:Long)=listOf(start to end) }' | Set-Content -LiteralPath "$fixture/app/src/main/java/com/tmw/companion/MainActivity.kt" -Encoding utf8
foreach($file in @('ReadingActivityStorage.kt','RecoveryExport.kt','UserSyncStorage.kt','SyncCompatibility.kt')) {
    Copy-Item -LiteralPath "$native/app/src/main/java/com/tmw/companion/$file" -Destination "$fixture/app/src/main/java/com/tmw/companion/$file" -Force
}
foreach($file in @('ReadingActivityStorageTest.kt','RecoveryExportTest.kt')) {
    Copy-Item -LiteralPath "$native/app/src/androidTest/java/com/tmw/companion/$file" -Destination "$fixture/app/src/androidTest/java/com/tmw/companion/$file" -Force
}
# A separate package, no Tauri/Rust build, no production data or connection credentials.
& "$native/gradlew.bat" -p $fixture assembleDebug assembleDebugAndroidTest --offline --no-daemon
if($LASTEXITCODE -ne 0) {throw 'Isolated native activity regressions failed.'}
# Direct instrumentation avoids fetching optional Gradle UTP orchestration packages.
& $adb -s $Serial install -r "$fixture/app/build/outputs/apk/debug/app-debug.apk"
if($LASTEXITCODE -ne 0) {throw 'Could not update the isolated test package.'}
& $adb -s $Serial install -r "$fixture/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
if($LASTEXITCODE -ne 0) {throw 'Could not update the isolated instrumentation package.'}
$result=& $adb -s $Serial shell am instrument -w com.tmw.activityfixture.test/androidx.test.runner.AndroidJUnitRunner
$result | Set-Content -LiteralPath "$fixture/instrumentation-result.txt" -Encoding utf8
$result
if($LASTEXITCODE -ne 0 -or ($result -join "`n") -notmatch 'OK \(6 tests\)') {throw 'Isolated native activity regressions did not pass.'}
