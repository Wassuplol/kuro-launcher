# Captures the launcher window to a PNG using a real screen copy.
# PrintWindow returns stale frames for WebView2 windows, so this grabs pixels.
param(
  [string]$Out = "shot.png",
  [string]$Process = "kuro-launcher"
)

Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
public class WinApi {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@

$proc = Get-Process $Process -ErrorAction Stop | Select-Object -First 1
$handle = $proc.MainWindowHandle
[WinApi]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 500

$rect = New-Object RECT
[WinApi]::GetWindowRect($handle, [ref]$rect) | Out-Null

$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top

$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
$bitmap.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

Write-Output "$Out ${width}x${height}"
