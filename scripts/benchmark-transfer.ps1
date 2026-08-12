param(
    [Parameter(Mandatory = $true)]
    [string]$IperfServer,
    [Parameter(Mandatory = $true)]
    [string]$SourceDirectory,
    [Parameter(Mandatory = $true)]
    [string]$ReceiveDirectory,
    [double]$TransferSeconds,
    [long]$FileBytes = 10GB,
    [int]$Runs = 3,
    [string]$Iperf = 'iperf3.exe',
    [switch]$PrepareFile
)

$ErrorActionPreference = 'Stop'
if ($Runs -lt 1) { throw 'Runs must be at least 1.' }
if ($FileBytes -lt 1MB) { throw 'FileBytes must be at least 1 MiB.' }
$iperfCommand = Get-Command $Iperf -ErrorAction SilentlyContinue
if (-not $iperfCommand) { throw "iperf3 was not found: $Iperf" }

New-Item -ItemType Directory -Force -Path $SourceDirectory, $ReceiveDirectory | Out-Null
$sourcePath = Join-Path $SourceDirectory 'transassist-benchmark.bin'
$receiveProbe = Join-Path $ReceiveDirectory 'transassist-write-probe.bin'

function Write-IncompressibleFile([string]$Path, [long]$Length) {
    $buffer = [byte[]]::new(8MB)
    $random = [Security.Cryptography.RandomNumberGenerator]::Create()
    $stream = [IO.FileStream]::new(
        $Path,
        [IO.FileMode]::Create,
        [IO.FileAccess]::Write,
        [IO.FileShare]::None,
        $buffer.Length,
        [IO.FileOptions]::SequentialScan
    )
    try {
        $remaining = $Length
        while ($remaining -gt 0) {
            $count = [int][Math]::Min($buffer.Length, $remaining)
            $random.GetBytes($buffer)
            $stream.Write($buffer, 0, $count)
            $remaining -= $count
        }
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
        $random.Dispose()
    }
}

function Measure-SequentialRead([string]$Path) {
    $buffer = [byte[]]::new(8MB)
    $stream = [IO.FileStream]::new(
        $Path,
        [IO.FileMode]::Open,
        [IO.FileAccess]::Read,
        [IO.FileShare]::Read,
        $buffer.Length,
        [IO.FileOptions]::SequentialScan
    )
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $bytes = 0L
    try {
        while (($count = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) { $bytes += $count }
    } finally {
        $stream.Dispose()
        $watch.Stop()
    }
    return ($bytes * 8.0 / $watch.Elapsed.TotalSeconds / 1MB)
}

function Measure-SequentialWrite([string]$Path, [long]$Length) {
    $buffer = [byte[]]::new(8MB)
    $random = [Security.Cryptography.RandomNumberGenerator]::Create()
    try {
        $random.GetBytes($buffer)
    } finally {
        $random.Dispose()
    }
    $stream = [IO.FileStream]::new(
        $Path,
        [IO.FileMode]::Create,
        [IO.FileAccess]::Write,
        [IO.FileShare]::None,
        $buffer.Length,
        [IO.FileOptions]::SequentialScan
    )
    $watch = [Diagnostics.Stopwatch]::StartNew()
    try {
        $remaining = $Length
        while ($remaining -gt 0) {
            $count = [int][Math]::Min($buffer.Length, $remaining)
            $stream.Write($buffer, 0, $count)
            $remaining -= $count
        }
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
        $watch.Stop()
    }
    return ($Length * 8.0 / $watch.Elapsed.TotalSeconds / 1MB)
}

if ($PrepareFile -or -not (Test-Path -LiteralPath $sourcePath) -or (Get-Item $sourcePath).Length -ne $FileBytes) {
    Write-Host "Generating $FileBytes bytes of incompressible data at $sourcePath"
    Write-IncompressibleFile $sourcePath $FileBytes
}

$iperfJson = & $iperfCommand.Source -c $IperfServer -P 4 -t 20 -J | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'iperf3 failed.' }
$networkMbps = $iperfJson.end.sum_received.bits_per_second / 1MB
$sourceReadMbps = Measure-SequentialRead $sourcePath
$receiveWriteMbps = Measure-SequentialWrite $receiveProbe $FileBytes
Remove-Item -LiteralPath $receiveProbe -Force
$bottleneckMbps = [Math]::Min($networkMbps, [Math]::Min($sourceReadMbps, $receiveWriteMbps))
$requiredMbps = $bottleneckMbps * 0.8

Write-Host ('Network:       {0:N1} Mbit/s' -f $networkMbps)
Write-Host ('Source read:   {0:N1} Mbit/s' -f $sourceReadMbps)
Write-Host ('Receive write: {0:N1} Mbit/s' -f $receiveWriteMbps)
Write-Host ('80% target:    {0:N1} Mbit/s' -f $requiredMbps)
Write-Host "Use the app to transfer $sourcePath $Runs times and pass each stable-stage duration as -TransferSeconds."

if ($TransferSeconds -gt 0) {
    $actualMbps = $FileBytes * 8.0 / $TransferSeconds / 1MB
    Write-Host ('Measured:      {0:N1} Mbit/s' -f $actualMbps)
    if ($actualMbps -lt $requiredMbps) {
        throw ('Performance acceptance failed: {0:N1} < {1:N1} Mbit/s' -f $actualMbps, $requiredMbps)
    }
    Write-Host 'Performance acceptance passed.'
}
