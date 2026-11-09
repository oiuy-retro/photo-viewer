# Shared by set-default-viewer.ps1 and undo-default-viewer.ps1.
# The current user's picture defaults. No administrator rights.

$script:PhotoViewerProgId = 'oiuy.PhotoViewer.1'
$script:PhotoViewerExeName = 'Photo Viewer.exe'
$script:PhotoViewerBackup = Join-Path $env:APPDATA 'Photo Viewer\default-viewer.json'

# Same picture types src/photo.rs lists.
$script:PhotoViewerExtensions = @(
    '.jpg', '.jpeg', '.jpe', '.jfif', '.png', '.gif', '.bmp', '.dib', '.tif', '.tiff',
    '.webp', '.ico', '.heic', '.heif', '.wdp', '.jxr', '.hdp', '.avif',
    '.dng', '.cr2', '.cr3', '.crw', '.nef', '.nrw', '.arw', '.srf', '.sr2', '.orf',
    '.rw2', '.raf', '.pef', '.ptx', '.srw', '.x3f', '.3fr', '.fff', '.iiq', '.mef',
    '.mos', '.mrw', '.erf', '.kdc', '.dcr', '.rwl', '.raw', '.gpr'
)

if (-not ('PhotoViewerUserChoice' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;

public static class PhotoViewerUserChoice {
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode)]
    static extern int RegOpenKeyEx(UIntPtr hKey, string sub, int opt, int sam, out UIntPtr result);
    [DllImport("advapi32.dll")]
    static extern int RegQueryInfoKey(UIntPtr hKey, IntPtr a, IntPtr b, IntPtr c, IntPtr d, IntPtr e, IntPtr f, IntPtr g, IntPtr h, IntPtr i, IntPtr j, out long lastWrite);
    [DllImport("advapi32.dll")]
    static extern int RegCloseKey(UIntPtr hKey);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode)]
    static extern int RegDeleteKeyW(UIntPtr hKey, string subKey);
    [DllImport("shell32.dll")]
    static extern void SHChangeNotify(int eventId, uint flags, IntPtr item1, IntPtr item2);
    [DllImport("Shlwapi.dll", CharSet = CharSet.Unicode)]
    static extern int AssocQueryString(int flags, int str, string assoc, string extra, StringBuilder output, ref int count);

    static readonly UIntPtr HKCU = (UIntPtr)0x80000001;
    const int KEY_READ = 0x20019;

    public static long LastWrite(string subKey) {
        UIntPtr handle;
        int rc = RegOpenKeyEx(HKCU, subKey, 0, KEY_READ, out handle);
        if (rc != 0) throw new InvalidOperationException("Could not open " + subKey + " (" + rc + ")");
        long fileTime;
        rc = RegQueryInfoKey(handle, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, out fileTime);
        RegCloseKey(handle);
        if (rc != 0) throw new InvalidOperationException("Could not read the time of " + subKey + " (" + rc + ")");
        return fileTime;
    }

    // The PowerShell drive and reg.exe cannot delete a UserChoice key.
    // RegDeleteKeyW can, for picture types. 0 = deleted, 2 = already gone.
    public static int DeleteKey(string subKey) {
        return RegDeleteKeyW(HKCU, subKey);
    }

    public static void NotifyAssociationsChanged() {
        SHChangeNotify(0x08000000, 0, IntPtr.Zero, IntPtr.Zero);
    }

    public static string QueryCommand(string extension) {
        var output = new StringBuilder(2048);
        int count = output.Capacity;
        int hr = AssocQueryString(0, 1, extension, null, output, ref count);
        if (hr != 0) return "";
        return output.ToString();
    }

    // Windows stores the user's default with a hash of the extension, the
    // account, the ProgId, and the key's time. Settings writes the same hash.
    public static string Hash(string extension, string sid, string progId, long keyFileTime) {
        DateTime written = DateTime.FromFileTimeUtc(keyFileTime);
        DateTime minute = new DateTime(written.Year, written.Month, written.Day, written.Hour, written.Minute, 0, DateTimeKind.Utc);
        long rounded = minute.ToFileTimeUtc();
        uint high = (uint)((ulong)rounded >> 32);
        uint low = (uint)(ulong)rounded;
        string experience = "User Choice set via Windows User Experience {D18B6DD5-6124-4341-9318-804003BAFA0B}";
        string text = (extension + sid + progId + high.ToString("x8") + low.ToString("x8") + experience).ToLowerInvariant();
        byte[] encoded = Encoding.Unicode.GetBytes(text);
        byte[] bytes = new byte[encoded.Length + 2];
        Buffer.BlockCopy(encoded, 0, bytes, 0, encoded.Length);
        byte[] md5;
        using (MD5 algorithm = MD5.Create()) md5 = algorithm.ComputeHash(bytes);
        uint md0 = BitConverter.ToUInt32(md5, 0);
        uint md1 = BitConverter.ToUInt32(md5, 4);
        uint[][] c0 = new uint[][] {
            new uint[] { md0 | 1, 0xCF98B111u, 0x87085B9Fu, 0x12CEB96Du, 0x257E1D83u },
            new uint[] { md1 | 1, 0xA27416F5u, 0xD38396FFu, 0x7C932B89u, 0xBFA49F69u }
        };
        uint[][] c1 = new uint[][] {
            new uint[] { md0 | 1, 0xEF0569FBu, 0x689B6B9Fu, 0x79F8A395u, 0xC3EFEA97u },
            new uint[] { md1 | 1, 0xC31713DBu, 0xDDCD1F0Fu, 0x59C3AF2Du, 0x35BD1EC9u }
        };
        uint h0 = 0, h1 = 0, h0Acc = 0, h1Acc = 0;
        int blocks = bytes.Length / 8;
        for (int i = 0; i < blocks; i++) {
            for (int j = 0; j < 2; j++) {
                uint input = BitConverter.ToUInt32(bytes, (i * 2 + j) * 4);
                uint[] left = c0[j];
                uint[] right = c1[j];
                h0 += input;
                h0 *= left[0];
                h0 = WordSwap(h0) * left[1];
                h0 = WordSwap(h0) * left[2];
                h0 = WordSwap(h0) * left[3];
                h0 = WordSwap(h0) * left[4];
                h0Acc += h0;
                h1 += input;
                h1 = WordSwap(h1) * right[1] + h1 * right[0];
                h1 = (h1 >> 16) * right[2] + h1 * right[3];
                h1 = WordSwap(h1) * right[4] + h1;
                h1Acc += h1;
            }
        }
        byte[] hash = new byte[8];
        Buffer.BlockCopy(BitConverter.GetBytes(h0 ^ h1), 0, hash, 0, 4);
        Buffer.BlockCopy(BitConverter.GetBytes(h0Acc ^ h1Acc), 0, hash, 4, 4);
        return Convert.ToBase64String(hash);
    }

    static uint WordSwap(uint value) {
        return (value >> 16) | (value << 16);
    }
}
'@
}

function Get-PhotoViewerSid {
    return [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
}

function Assert-PhotoViewerChoiceScheme {
    $latest = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.jpg\UserChoiceLatest'
    if (Test-Path -LiteralPath $latest) {
        throw 'This Windows build keeps defaults in UserChoiceLatest. Use Settings > Apps > Default apps for each picture type.'
    }
}

function Get-PhotoViewerChoice([string]$Extension) {
    $key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$Extension\UserChoice"
    if (-not (Test-Path -LiteralPath $key)) { return $null }
    $item = Get-ItemProperty -LiteralPath $key
    return @{ ProgId = [string]$item.ProgId; Hash = [string]$item.Hash }
}

function Remove-PhotoViewerChoiceKey([string]$Extension) {
    $subKey = "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$Extension\UserChoice"
    $rc = [PhotoViewerUserChoice]::DeleteKey($subKey)
    if ($rc -eq 0 -or $rc -eq 2) { return }
    throw "Windows blocked changing the default for $Extension. Use Settings > Apps > Default apps for that type."
}

function Set-PhotoViewerChoice([string]$Extension, [string]$ProgId) {
    $parent = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$Extension"
    if (-not (Test-Path -LiteralPath $parent)) {
        New-Item -Path $parent -Force | Out-Null
    }
    $sid = Get-PhotoViewerSid
    $subKey = "Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$Extension\UserChoice"
    $full = 'HKEY_CURRENT_USER\' + $subKey
    for ($try = 1; $try -le 3; $try++) {
        Remove-PhotoViewerChoiceKey $Extension
        # Hash the minute the key is about to be written in, then write both
        # values. Writing the hash updates the key time, so it is computed first.
        $hash = [PhotoViewerUserChoice]::Hash($Extension, $sid, $ProgId, [DateTime]::UtcNow.ToFileTime())
        [Microsoft.Win32.Registry]::SetValue($full, 'ProgId', $ProgId)
        [Microsoft.Win32.Registry]::SetValue($full, 'Hash', $hash)
        $checked = [PhotoViewerUserChoice]::Hash($Extension, $sid, $ProgId, [PhotoViewerUserChoice]::LastWrite($subKey))
        if ($checked -eq $hash) { return }
    }
    throw "Windows did not keep the default for $Extension."
}

function Remove-EmptyExtensionKey([string]$Extension) {
    $path = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\$Extension"
    if (-not (Test-Path -LiteralPath $path)) { return }
    $item = Get-Item -LiteralPath $path
    $children = @(Get-ChildItem -LiteralPath $path -ErrorAction SilentlyContinue)
    $names = @($item.GetValueNames() | Where-Object { -not [string]::IsNullOrEmpty($_) })
    if ($children.Count -eq 0 -and $names.Count -eq 0) {
        Remove-Item -LiteralPath $path -Force
    }
}

function Restore-PhotoViewerChoice([string]$Extension, $Saved) {
    if (-not $Saved.HadChoice) {
        Remove-PhotoViewerChoiceKey $Extension
        Remove-EmptyExtensionKey $Extension
        return
    }
    # The hash is tied to the key's current time, so the saved hash cannot be
    # written back. Recompute it for the ProgId that was there before.
    Set-PhotoViewerChoice $Extension ([string]$Saved.ProgId)
}

function Register-PhotoViewerProgId([string]$ExePath) {
    $classKey = "HKCU:\Software\Classes\$script:PhotoViewerProgId"
    New-Item -Path $classKey -Force | Out-Null
    Set-Item -LiteralPath $classKey -Value 'Photo Viewer'
    New-Item -Path "$classKey\DefaultIcon" -Force | Out-Null
    Set-Item -LiteralPath "$classKey\DefaultIcon" -Value "`"$ExePath`",0"
    New-Item -Path "$classKey\shell\open\command" -Force | Out-Null
    Set-Item -LiteralPath "$classKey\shell\open\command" -Value "`"$ExePath`" `"%1`""

    $appKey = 'HKCU:\Software\Classes\Applications\Photo Viewer.exe'
    $commandKey = "$appKey\shell\open\command"
    $previous = $null
    if (Test-Path -LiteralPath $commandKey) {
        $previous = (Get-Item -LiteralPath $commandKey).GetValue('')
    }
    New-Item -Path $commandKey -Force | Out-Null
    Set-ItemProperty -LiteralPath $appKey -Name FriendlyAppName -Value 'Photo Viewer' -Force
    Set-Item -LiteralPath $commandKey -Value "`"$ExePath`" `"%1`""
    return $previous
}

function Unregister-PhotoViewerProgId {
    $classKey = "HKCU:\Software\Classes\$script:PhotoViewerProgId"
    if (Test-Path -LiteralPath $classKey) {
        Remove-Item -LiteralPath $classKey -Recurse -Force
    }
    $toast = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts'
    if (Test-Path -LiteralPath $toast) {
        $item = Get-Item -LiteralPath $toast
        foreach ($name in @($item.GetValueNames())) {
            if ($name.StartsWith("$script:PhotoViewerProgId" + '_')) {
                Remove-ItemProperty -LiteralPath $toast -Name $name -Force
            }
        }
    }
}

function Resolve-PhotoViewerExe([string]$Directory) {
    if ([string]::IsNullOrWhiteSpace($Directory)) {
        $Directory = Read-Host 'Folder that contains Photo Viewer.exe'
    }
    $Directory = $Directory.Trim().Trim('"')
    if ([string]::IsNullOrWhiteSpace($Directory)) {
        throw 'Pass the folder that contains Photo Viewer.exe.'
    }
    if (Test-Path -LiteralPath $Directory -PathType Leaf) {
        $exe = (Resolve-Path -LiteralPath $Directory).Path
    } else {
        $exe = Join-Path $Directory $script:PhotoViewerExeName
        if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
            throw "No Photo Viewer.exe in $Directory"
        }
        $exe = (Resolve-Path -LiteralPath $exe).Path
    }
    if ([IO.Path]::GetFileName($exe) -ne $script:PhotoViewerExeName) {
        throw "Expected Photo Viewer.exe, got $exe"
    }
    return $exe
}

function Read-PhotoViewerBackup {
    if (-not (Test-Path -LiteralPath $script:PhotoViewerBackup)) { return $null }
    return Get-Content -LiteralPath $script:PhotoViewerBackup -Raw | ConvertFrom-Json
}

function Save-PhotoViewerBackup($Backup) {
    $folder = Split-Path -Parent $script:PhotoViewerBackup
    if (-not (Test-Path -LiteralPath $folder)) {
        New-Item -ItemType Directory -Path $folder -Force | Out-Null
    }
    $json = $Backup | ConvertTo-Json -Depth 6
    Set-Content -LiteralPath $script:PhotoViewerBackup -Value $json -Encoding UTF8
}

function Get-PhotoViewerSnapshot([string]$ExePath, $PreviousCommand, [bool]$ApplicationKeyExisted) {
    $extensions = @()
    foreach ($extension in $script:PhotoViewerExtensions) {
        $choice = Get-PhotoViewerChoice $extension
        if ($null -eq $choice) {
            $extensions += @{ Ext = $extension; HadChoice = $false; ProgId = ''; Hash = '' }
        } else {
            $extensions += @{ Ext = $extension; HadChoice = $true; ProgId = $choice.ProgId; Hash = $choice.Hash }
        }
    }
    return @{
        Exe = $ExePath
        ProgId = $script:PhotoViewerProgId
        SavedAt = [DateTime]::UtcNow.ToString('o')
        PreviousApplicationCommand = $PreviousCommand
        ApplicationKeyExisted = $ApplicationKeyExisted
        Extensions = $extensions
    }
}

function Set-PhotoViewerDefaults([string]$Directory) {
    Assert-PhotoViewerChoiceScheme
    $exe = Resolve-PhotoViewerExe $Directory
    $appKey = 'HKCU:\Software\Classes\Applications\Photo Viewer.exe'
    $applicationKeyExisted = Test-Path -LiteralPath $appKey
    $previousCommand = Register-PhotoViewerProgId $exe
    $existing = Read-PhotoViewerBackup
    if ($null -eq $existing) {
        # Snapshot before any default is changed. A second run keeps this file.
        $commandForBackup = $previousCommand
        if ($null -eq $commandForBackup) { $commandForBackup = '' }
        Save-PhotoViewerBackup (Get-PhotoViewerSnapshot $exe $commandForBackup $applicationKeyExisted)
    } else {
        $existing.Exe = $exe
        Save-PhotoViewerBackup $existing
    }

    $toast = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ApplicationAssociationToasts'
    if (-not (Test-Path -LiteralPath $toast)) {
        New-Item -Path $toast -Force | Out-Null
    }
    $failed = @()
    foreach ($extension in $script:PhotoViewerExtensions) {
        try {
            Set-PhotoViewerChoice $extension $script:PhotoViewerProgId
            $toastName = $script:PhotoViewerProgId + '_' + $extension
            New-ItemProperty -LiteralPath $toast -Name $toastName -Value 0 -PropertyType DWord -Force | Out-Null
        } catch {
            $failed += $extension
        }
    }
    [PhotoViewerUserChoice]::NotifyAssociationsChanged()
    if ($failed.Count -eq $script:PhotoViewerExtensions.Count) {
        throw 'Windows blocked changing these picture defaults. Set them in Settings > Apps > Default apps. Undo with scripts\undo-default-viewer.ps1'
    }
    if ($failed.Count -eq 0) {
        Write-Output 'Photo Viewer is the default for picture types, using'
    } else {
        Write-Output 'Photo Viewer is the default for the picture types Windows allowed, using'
    }
    Write-Output $exe
    Write-Output ''
    Write-Output 'Undo with scripts\undo-default-viewer.ps1'
    Write-Output 'The previous defaults are in:'
    Write-Output $script:PhotoViewerBackup
    if ($failed.Count -gt 0) {
        Write-Output ''
        Write-Output 'Windows kept these types unchanged:'
        $failed | ForEach-Object { Write-Output $_ }
    }
}

function Undo-PhotoViewerDefaults {
    $backup = Read-PhotoViewerBackup
    if ($null -eq $backup) {
        throw "No saved defaults at $($script:PhotoViewerBackup). Settings > Apps > Default apps changes one type at a time."
    }
    $skipped = @()
    foreach ($saved in @($backup.Extensions)) {
        $current = Get-PhotoViewerChoice $saved.Ext
        $ours = $false
        if ($null -eq $current) {
            $ours = -not $saved.HadChoice
        } else {
            $ours = ($current.ProgId -eq $script:PhotoViewerProgId)
        }
        if (-not $ours) {
            $skipped += $saved.Ext
            continue
        }
        Restore-PhotoViewerChoice $saved.Ext $saved
    }
    $appCommand = 'HKCU:\Software\Classes\Applications\Photo Viewer.exe\shell\open\command'
    if (-not $backup.ApplicationKeyExisted) {
        Remove-Item -LiteralPath 'HKCU:\Software\Classes\Applications\Photo Viewer.exe' -Recurse -Force -ErrorAction SilentlyContinue
    } elseif (Test-Path -LiteralPath $appCommand) {
        Set-Item -LiteralPath $appCommand -Value ([string]$backup.PreviousApplicationCommand)
    }
    Unregister-PhotoViewerProgId
    Remove-Item -LiteralPath $script:PhotoViewerBackup -Force
    [PhotoViewerUserChoice]::NotifyAssociationsChanged()
    Write-Output 'Restored the picture defaults from before Photo Viewer was set.'
    if ($skipped.Count -gt 0) {
        Write-Output ''
        Write-Output 'Left these alone because they are no longer Photo Viewer:'
        $skipped | ForEach-Object { Write-Output $_ }
    }
}
