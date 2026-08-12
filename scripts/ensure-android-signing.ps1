param(
    [string]$Keytool,
    [string]$KeystorePath,
    [string]$PropertiesPath
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $KeystorePath) {
    $KeystorePath = Join-Path $repoRoot 'app\android\transassist-release.jks'
}
if (-not $PropertiesPath) {
    $PropertiesPath = Join-Path $repoRoot 'app\android\key.properties'
}

$keystoreExists = Test-Path -LiteralPath $KeystorePath
$propertiesExist = Test-Path -LiteralPath $PropertiesPath
if ($keystoreExists -xor $propertiesExist) {
    throw 'Android signing is incomplete. Restore both transassist-release.jks and key.properties, or remove both and generate a new identity.'
}
if ($keystoreExists) {
    Write-Host "Android release signing already exists at $KeystorePath"
    exit 0
}

if (-not $Keytool) {
    $command = Get-Command keytool.exe -ErrorAction SilentlyContinue
    if ($command) {
        $Keytool = $command.Source
    } elseif ($env:JAVA_HOME) {
        $Keytool = Join-Path $env:JAVA_HOME 'bin\keytool.exe'
    }
}
if (-not $Keytool -or -not (Test-Path -LiteralPath $Keytool)) {
    throw 'keytool.exe was not found. Install JDK 17 or pass -Keytool.'
}

function New-RandomSecret {
    $bytes = [byte[]]::new(32)
    $random = [Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $random.GetBytes($bytes)
    } finally {
        $random.Dispose()
    }
    return [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', 'A').Replace('/', 'B')
}

$password = New-RandomSecret
$alias = 'transassist'
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $KeystorePath) | Out-Null
& $Keytool -genkeypair -v `
    -keystore $KeystorePath `
    -storetype PKCS12 `
    -storepass $password `
    -keypass $password `
    -alias $alias `
    -keyalg RSA `
    -keysize 4096 `
    -validity 10000 `
    -dname 'CN=Transfer Assistant, OU=Release, O=TransAssist, L=Shanghai, ST=Shanghai, C=CN'
if ($LASTEXITCODE -ne 0) {
    throw "keytool failed with exit code $LASTEXITCODE"
}

$properties = @(
    'storeFile=transassist-release.jks'
    "storePassword=$password"
    "keyAlias=$alias"
    "keyPassword=$password"
)
[IO.File]::WriteAllLines($PropertiesPath, $properties, [Text.UTF8Encoding]::new($false))
Write-Host 'Created the persistent Android release key. Back up both files; losing them prevents signing upgrades.'
Write-Host $KeystorePath
Write-Host $PropertiesPath
