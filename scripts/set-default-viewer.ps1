# Set Photo Viewer as the current user's default for the picture types it opens.
# No administrator rights. Pass the folder that contains Photo Viewer.exe,
# or run it and type that folder when asked.
param(
    [Parameter(Position = 0)]
    [string]$Directory
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'default-viewer-common.ps1')
Set-PhotoViewerDefaults -Directory $Directory
