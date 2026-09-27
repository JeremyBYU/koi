# Installs koi on Windows: the latest release's archive, checked against the release's
# SHA256SUMS, into %LOCALAPPDATA%\Programs\koi, which is added to your PATH.
#
#   irm https://jeremybyu.github.io/koi/install.ps1 | iex
#
# $env:KOI_VERSION picks a release, such as 0.5.0, instead of the latest. Windows support is
# experimental.

# In its own scope, so `irm | iex` leaves nothing set in your session.
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'

    $repo = 'https://github.com/JeremyBYU/koi'
    $dir = Join-Path $env:LOCALAPPDATA 'Programs\koi'

    if ($env:KOI_VERSION) {
        $version = $env:KOI_VERSION.TrimStart('v')
    } else {
        $version = (Invoke-RestMethod 'https://api.github.com/repos/JeremyBYU/koi/releases/latest').tag_name.TrimStart('v')
    }

    # The one Windows build is x86_64; Windows on ARM runs it through emulation.
    $archive = "koi-$version-windows-x86_64.zip"
    $work = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        Write-Host "Downloading koi $version"
        Invoke-WebRequest "$repo/releases/download/v$version/$archive" -OutFile (Join-Path $work $archive)
        Invoke-WebRequest "$repo/releases/download/v$version/SHA256SUMS" -OutFile (Join-Path $work 'SHA256SUMS')

        $line = Get-Content (Join-Path $work 'SHA256SUMS') | Where-Object { $_ -match " $([regex]::Escape($archive))$" }
        $expected = if ($line) { ($line -split ' ')[0] } else { '' }
        $actual = (Get-FileHash (Join-Path $work $archive) -Algorithm SHA256).Hash.ToLower()
        if (-not $expected -or $expected -ne $actual) {
            throw "The download does not match the release's SHA256SUMS, so nothing was installed."
        }

        Expand-Archive (Join-Path $work $archive) -DestinationPath $work
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        Copy-Item (Join-Path $work "koi-$version-windows-x86_64\koi.exe") (Join-Path $dir 'koi.exe') -Force
        Write-Host "Installed koi $version to $dir\koi.exe"
    } finally {
        Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
    }

    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($path -split ';') -notcontains $dir) {
        [Environment]::SetEnvironmentVariable('Path', ($(if ($path) { "$path;$dir" } else { $dir })), 'User')
        Write-Host "Added $dir to your PATH. Open a new terminal, then run: koi"
    } else {
        Write-Host 'Run it with: koi'
    }
}
