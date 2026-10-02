$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$previousRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
$previousToolchain = $env:CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc
$toolchain = Join-Path $PSScriptRoot 'windows-static-crt.cmake'
$toolchainHash = (Get-FileHash -LiteralPath $toolchain -Algorithm SHA256).Hash.Substring(0, 12)
$targetDirectory = Join-Path $projectRoot "target/package-$toolchainHash"
Push-Location -LiteralPath $projectRoot
try {
    $env:CARGO_ENCODED_RUSTFLAGS = '-C' + [char]31 + 'target-feature=+crt-static'
    $env:CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc = $toolchain
    & cargo build --release --target x86_64-pc-windows-msvc --target-dir $targetDirectory --bin zeff-rhythm-app
    if ($LASTEXITCODE -ne 0) { throw 'Build failed.' }
    $packageDirectory = Join-Path $projectRoot 'dist/Zeff Rhythm'
    New-Item -ItemType Directory -Path $packageDirectory -Force | Out-Null
    $executable = Join-Path $packageDirectory 'Zeff Rhythm.exe'
    $builtExecutable = Join-Path $targetDirectory 'x86_64-pc-windows-msvc/release/zeff-rhythm-app.exe'
    Copy-Item -LiteralPath $builtExecutable -Destination $executable -Force
    Compress-Archive -LiteralPath $executable -DestinationPath 'dist/Zeff-Rhythm-Windows.zip' -Force
    Write-Output $executable
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $previousRustFlags
    $env:CMAKE_TOOLCHAIN_FILE_x86_64_pc_windows_msvc = $previousToolchain
    Pop-Location
}
