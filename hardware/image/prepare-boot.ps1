# Copies the wall-player recipe onto a Raspberry Pi OS boot partition.
#
# Flash Raspberry Pi OS Lite (64-bit) for the Pi 4 with Imager first.
# The username must be admin, with SSH and Wi-Fi turned on. When Imager
# finishes it leaves the boot partition mounted as bootfs. Then, from this
# directory:
#
#   powershell -File .\prepare-boot.ps1
#
# Eject the card, boot the Pi, and leave it on the network. The first boot
# installs cage, locks the system partition, and reboots into the player.
# Settings are stored in djusb-data.img on this boot partition so a power
# cut keeps them.

param(
    [string]$Boot,
    [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'

function ConvertTo-UnixText([string]$Text) {
    $unix = $Text -replace "`r`n", "`n" -replace "`r", "`n"
    if (-not $unix.EndsWith("`n")) {
        $unix += "`n"
    }
    return $unix
}

function Add-FirstBootCommand([string]$Text) {
    $command = '  - bash /boot/firmware/djusb-firstboot.sh'
    $unix = ConvertTo-UnixText $Text
    if ($unix -match 'djusb-firstboot\.sh') {
        return $unix
    }
    $lines = $unix.TrimEnd("`n") -split "`n", -1
    $out = New-Object System.Collections.Generic.List[string]
    $inRun = $false
    $inserted = $false
    foreach ($line in $lines) {
        if (-not $inserted -and $inRun -and $line -match '^[A-Za-z0-9_]+:') {
            $out.Add($command)
            $inserted = $true
            $inRun = $false
        }
        if ($line -match '^runcmd:\s*$') {
            $inRun = $true
        }
        $out.Add($line)
    }
    if (-not $inserted) {
        if (-not $inRun) {
            $out.Add('runcmd:')
        }
        $out.Add($command)
    }
    return (($out -join "`n") + "`n")
}

function Add-BootTokens([string]$Text) {
    $line = ($Text -replace "`r|`n", ' ').Trim()
    foreach ($token in @('quiet splash', 'video=DSI-1:panel_orientation=left_side_up')) {
        if ($line -notlike "*$token*") {
            $line = "$line $token"
        }
    }
    return $line + "`n"
}

function Write-UnixFile([string]$Path, [string]$Text) {
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, (ConvertTo-UnixText $Text), $utf8)
}

if ($SelfTest) {
    $withRun = "hostname: djmp3`nruncmd:`n  - echo hi`npackages:`n  - avahi-daemon`n"
    $patched = Add-FirstBootCommand $withRun
    if ($patched -notmatch '(?s)runcmd:\n  - echo hi\n  - bash /boot/firmware/djusb-firstboot\.sh\npackages:') {
        throw "first-boot command was not placed last in runcmd"
    }
    $again = Add-FirstBootCommand $patched
    if (($again.Split("`n") | Where-Object { $_ -match 'djusb-firstboot' }).Count -ne 1) {
        throw "first-boot command was inserted twice"
    }
    $bare = Add-FirstBootCommand "hostname: djmp3`n"
    if ($bare -notmatch '(?s)hostname: djmp3\nruncmd:\n  - bash /boot/firmware/djusb-firstboot\.sh\n') {
        throw "first-boot command was not appended"
    }
    $cmdline = Add-BootTokens "console=tty1 rootwait quiet splash`n"
    if ($cmdline -ne "console=tty1 rootwait quiet splash video=DSI-1:panel_orientation=left_side_up`n") {
        throw "boot line was $cmdline"
    }
    $twice = Add-BootTokens $cmdline
    if ($twice -ne $cmdline) {
        throw "boot tokens were duplicated"
    }
    Write-Output 'prepare-boot checks passed'
    exit 0
}

if (-not $Boot) {
    $volumes = @(Get-Volume -ErrorAction SilentlyContinue | Where-Object { $_.FileSystemLabel -eq 'bootfs' })
    if ($volumes.Count -ne 1) {
        throw "Expected one drive named bootfs, found $($volumes.Count). Pass -Boot E:\ after Imager finishes."
    }
    $Boot = "$($volumes[0].DriveLetter):\"
}

$bootRoot = $Boot.TrimEnd('\')
if (-not (Test-Path (Join-Path $bootRoot 'cmdline.txt'))) {
    throw "$bootRoot does not look like a Raspberry Pi boot partition (no cmdline.txt)."
}

$userData = Join-Path $bootRoot 'user-data'
if (-not (Test-Path $userData)) {
    throw "$bootRoot has no user-data. Flash Lite again and set the username, Wi-Fi, and SSH in Imager."
}
$userText = [System.IO.File]::ReadAllText($userData)
if ($userText -notmatch '(?m)^\s*-\s+name:\s+admin\s*$' -and $userText -notmatch '(?m)^name:\s+admin\s*$') {
    throw "The Imager user is not admin. The player service signs in as admin. Flash again with that username."
}

$here = $PSScriptRoot
$binary = Join-Path $here '..\..\target\aarch64-unknown-linux-gnu\release\djusb-device'
if (-not (Test-Path $binary)) {
    throw "Missing $binary. Build it with: cross build --release -p djusb-device --target aarch64-unknown-linux-gnu"
}

Copy-Item $binary (Join-Path $bootRoot 'djusb-device') -Force
foreach ($name in @('djusb-kiosk', 'djusb-firstboot.sh', 'djusb.service')) {
    $source = Join-Path $here $name
    Write-UnixFile (Join-Path $bootRoot $name) ([System.IO.File]::ReadAllText($source))
}

$usbDest = Join-Path $bootRoot 'usb-mount'
New-Item -ItemType Directory -Force -Path $usbDest | Out-Null
foreach ($name in @('mount_options.conf', '99-djusb-automount.rules', 'djusb-mount@.service', 'djusb-mount', 'install.sh')) {
    $source = Join-Path $here "..\usb-mount\$name"
    if (-not (Test-Path $source)) {
        throw "Missing $source"
    }
    Write-UnixFile (Join-Path $usbDest $name) ([System.IO.File]::ReadAllText($source))
}

$cmdPath = Join-Path $bootRoot 'cmdline.txt'
Write-UnixFile $cmdPath (Add-BootTokens ([System.IO.File]::ReadAllText($cmdPath)))
Write-UnixFile $userData (Add-FirstBootCommand $userText)

foreach ($name in @('djusb-device', 'djusb-kiosk', 'djusb-firstboot.sh', 'djusb.service')) {
    if (-not (Test-Path (Join-Path $bootRoot $name))) {
        throw "Copy to $bootRoot failed: $name is missing."
    }
}
foreach ($name in @('mount_options.conf', '99-djusb-automount.rules', 'djusb-mount@.service', 'djusb-mount', 'install.sh')) {
    if (-not (Test-Path (Join-Path $usbDest $name))) {
        throw "Copy to $bootRoot failed: usb-mount/$name is missing."
    }
}

Write-Output "Boot partition $bootRoot is ready. Eject the card and power the Pi. The first boot needs Wi-Fi, then reboots into the player."
