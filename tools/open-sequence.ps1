# Captures the launcher repeatedly during the opening, recording the window rect
# with each frame, so a mid-transition frame can be identified rather than
# guessed at. CDP screenshots are useless here: capturing a surface that is
# being resized every 8 ms returns an empty buffer.
param(
  [string]$Prefix = "shots/open-seq",
  [int]$Frames = 16,
  [int]$GapMs = 130
)

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public struct RECT { public int Left, Top, Right, Bottom; }
public class SeqWin {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
}
"@

for ($i = 0; $i -lt $Frames; $i++) {
  $proc = Get-Process kuro-launcher -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $proc -or $proc.MainWindowHandle -eq 0) { continue }

  $rect = New-Object RECT
  [SeqWin]::GetWindowRect($proc.MainWindowHandle, [ref]$rect) | Out-Null
  $w = $rect.Right - $rect.Left
  $h = $rect.Bottom - $rect.Top
  if ($w -le 0 -or $h -le 0) { continue }

  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bmp.Size)
  $out = "{0}-{1:00}.png" -f $Prefix, $i
  $bmp.Save((Join-Path (Get-Location) $out), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()

  Write-Output ("{0} {1}x{2}" -f $out, $w, $h)
  Start-Sleep -Milliseconds $GapMs
}
