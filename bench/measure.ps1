# ==============================================================================
# Whole-application footprint benchmark for Hypatia.
#
#   .\bench\measure.ps1 [-Exe target\release\hypatia.exe] [-Seconds 20] [-Runs 3]
#
# Launches the binary in each scenario, lets it settle, then samples CPU time
# and memory over the measurement window. A single run is noisy (a desktop app
# shares the machine with everything else), so each scenario is run several
# times and the median is reported, with the min-max CPU range alongside.
# Close League before running so the "idle" scenario is comparable.
# ==============================================================================
param(
    [string]$Exe = "target\release\hypatia.exe",
    [int]$Seconds = 20,
    [int]$WarmupSeconds = 6,
    [int]$Runs = 3
)
$ErrorActionPreference = "Stop"

$scenarios = @(
    @{ Name = "idle (League closed)";        Args = @("--no-remote") },
    @{ Name = "demo live game";              Args = @("--demo", "--phase", "live", "--no-remote") },
    @{ Name = "demo + telemetry + remote";   Args = @("--demo", "--demo-rate", "20", "--telemetry", "--remote") }
)

function Median([double[]]$values) {
    $sorted = $values | Sort-Object
    return $sorted[[int][math]::Floor(($sorted.Count - 1) / 2)]
}

$exePath = (Resolve-Path $Exe).Path
$results = @()

foreach ($s in $scenarios) {
    $cpu = @(); $ws = @(); $private = @(); $startup = @(); $threads = @()
    foreach ($run in 1..$Runs) {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $p = Start-Process -FilePath $exePath -ArgumentList $s.Args -PassThru
        while ($p.MainWindowHandle -eq 0 -and $sw.Elapsed.TotalSeconds -lt 20) {
            Start-Sleep -Milliseconds 20
            $p.Refresh()
        }
        $startup += $sw.Elapsed.TotalMilliseconds

        Start-Sleep -Seconds $WarmupSeconds
        $p.Refresh()
        $cpu0 = $p.TotalProcessorTime.TotalMilliseconds
        Start-Sleep -Seconds $Seconds
        $p.Refresh()
        $cpu += ($p.TotalProcessorTime.TotalMilliseconds - $cpu0) / $Seconds
        $ws += $p.WorkingSet64 / 1MB
        $private += $p.PrivateMemorySize64 / 1MB
        $threads += $p.Threads.Count

        Stop-Process -Id $p.Id -Force
        Start-Sleep -Milliseconds 800
    }

    $results += [pscustomobject]@{
        Scenario      = $s.Name
        StartupMs     = [math]::Round((Median $startup))
        CpuMsPerSec   = [math]::Round((Median $cpu), 1)
        CpuRange      = "{0:N1}-{1:N1}" -f ($cpu | Measure-Object -Minimum).Minimum, ($cpu | Measure-Object -Maximum).Maximum
        CpuPctOneCore = [math]::Round((Median $cpu) / 10, 2)
        WorkingSetMB  = [math]::Round((Median $ws), 1)
        PrivateMB     = [math]::Round((Median $private), 1)
        Threads       = [math]::Round((Median $threads))
    }
}

"Binary: $exePath ($([math]::Round((Get-Item $exePath).Length / 1MB, 2)) MB) - $([Environment]::ProcessorCount) logical cores - median of $Runs runs x ${Seconds}s"
$results | Format-Table -AutoSize | Out-String -Width 200
