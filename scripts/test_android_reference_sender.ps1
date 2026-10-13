param(
    [Parameter(Mandatory)][ValidatePattern('^emulator-[0-9]+$')][string]$ReceiverSerial,
    [Parameter(Mandatory)][ValidatePattern('^emulator-[0-9]+$')][string]$SenderSerial,
    [Parameter(Mandatory)][string]$Adb,
    [switch]$Ui,
    [switch]$Capture,
    [switch]$CaptureVideoOnly
)
$ErrorActionPreference = 'Stop'
if ($ReceiverSerial -eq $SenderSerial) { throw 'Use two disposable emulators, not owner devices.' }
$taskRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$runDirectory = Join-Path $taskRoot ('build/reference-run-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $runDirectory | Out-Null
$receiverLog = Join-Path $runDirectory 'receiver.txt'
$senderLog = Join-Path $runDirectory 'sender.txt'
$receiverProcess = $null
$senderProcess = $null
$port = $null
try {
    foreach ($serial in @($ReceiverSerial, $SenderSerial)) {
        if ((& $Adb -s $serial shell getprop sys.boot_completed).Trim() -ne '1') { throw 'Emulator is not booted.' }
    }
    $receiverMethod = if ($CaptureVideoOnly) { 'testReceivesVideoOnlyCapture' } elseif ($Capture) { 'testReceivesActualScreenCapture' } else { 'testReceivesSeparateSenderProcess' }
    $receiverProcess = Start-Process -FilePath $Adb -ArgumentList @('-s', $ReceiverSerial, 'shell', 'am', 'instrument', '-w', '-e', 'class', "dev.frameark.receiver.ReferenceReceiverTest#$receiverMethod", 'dev.frameark.receiver.test/android.test.InstrumentationTestRunner') -WindowStyle Hidden -PassThru -RedirectStandardOutput $receiverLog -RedirectStandardError (Join-Path $runDirectory 'receiver-error.txt')
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        & $Adb -s $ReceiverSerial shell run-as dev.frameark.receiver test -f files/reference-ticket.txt 2>$null
        $ready = $LASTEXITCODE -eq 0
        if (!$ready) { Start-Sleep -Milliseconds 100 }
    } while (!$ready -and [DateTime]::UtcNow -lt $deadline)
    if (!$ready) { throw 'Receiver ticket was not ready within 15 seconds.' }
    $ticketText = & $Adb -s $ReceiverSerial exec-out run-as dev.frameark.receiver cat files/reference-ticket.txt
    if ($ticketText.Count -ne 3 -or $ticketText[1] -notmatch '^10\.0\.2\.2:([0-9]+)$') { throw 'Unexpected emulator fixture.' }
    $port = [int]$Matches[1]
    & $Adb -s $ReceiverSerial emu redir add "udp:${port}:${port}" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'UDP redirection failed.' }
    & $Adb -s $SenderSerial shell run-as dev.frameark.sender mkdir -p files
    ($ticketText -join "`n") | & $Adb -s $SenderSerial shell run-as dev.frameark.sender tee files/reference-ticket.txt > $null
    if ($LASTEXITCODE -ne 0) { throw 'Ticket transfer failed.' }
    & $Adb -s $ReceiverSerial exec-out run-as dev.frameark.receiver cat files/reference-pin.txt | & $Adb -s $SenderSerial shell run-as dev.frameark.sender tee files/reference-pin.txt > $null
    if ($LASTEXITCODE -ne 0) { throw 'PIN transfer failed.' }
    $method = if ($Ui) { 'testUiStreamsAndStops' } else { 'testPinnedReferenceStream' }
    $senderTest = if ($CaptureVideoOnly) { 'dev.frameark.sender.ScreenCaptureTest#testVideoOnlyStopsOnLock' } elseif ($Capture) { 'dev.frameark.sender.ScreenCaptureTest#testCapturesOsScreenAndPlaybackAudio' } else { "dev.frameark.sender.ReferenceSenderTest#$method" }
    $senderProcess = Start-Process -FilePath $Adb -ArgumentList @('-s', $SenderSerial, 'shell', 'am', 'instrument', '-w', '-e', 'class', $senderTest, 'dev.frameark.sender.test/android.test.InstrumentationTestRunner') -WindowStyle Hidden -PassThru -RedirectStandardOutput $senderLog -RedirectStandardError (Join-Path $runDirectory 'sender-error.txt')
    $deadline = [DateTime]::UtcNow.AddSeconds(110)
    while ((!$senderProcess.HasExited -or !$receiverProcess.HasExited) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
    foreach ($process in @($senderProcess, $receiverProcess)) { if (!$process.HasExited) { throw 'Instrumentation deadline exceeded.' } }
    foreach ($path in @($senderLog, $receiverLog)) {
        $result = Get-Content -LiteralPath $path -Raw
        Write-Output $result
        if ($result -notmatch 'OK \(1 test\)' -or $result -match 'FAILURES|INSTRUMENTATION_FAILED') { throw 'Instrumentation assertion failed.' }
    }
    Write-Output "Evidence: $runDirectory"
} finally {
    # Exact disposable test targets only; never connect to a physical serial.
    foreach ($process in @($senderProcess, $receiverProcess)) { if ($null -ne $process -and !$process.HasExited) { Stop-Process -Id $process.Id } }
    if ($null -ne $port) { & $Adb -s $ReceiverSerial emu redir del "udp:$port" | Out-Null }
    foreach ($target in @(@($ReceiverSerial, 'dev.frameark.receiver'), @($SenderSerial, 'dev.frameark.sender'))) {
        & $Adb -s $target[0] shell am force-stop $target[1]
        & $Adb -s $target[0] shell run-as $target[1] rm -f files/reference-ticket.txt files/reference-pin.txt
    }
}
