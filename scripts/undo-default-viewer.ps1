# Put back the picture defaults saved by set-default-viewer.ps1.
# Settings > Apps > Default apps can change one type at a time. It does not
# undo the whole set in one step, which is what this script does.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'default-viewer-common.ps1')
Undo-PhotoViewerDefaults
