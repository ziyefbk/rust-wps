$ErrorActionPreference = 'Stop'
$workspace = Split-Path -Parent $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'corpus.json') -Raw | ConvertFrom-Json
$corpusDirectory = Join-Path $workspace 'tmp/corpus'
New-Item -ItemType Directory -Force -Path $corpusDirectory | Out-Null
Push-Location $workspace
try {
    cargo build --locked
    if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed' }
    $executable = Join-Path $workspace 'target/debug/rust-wps.exe'
    $reports = @{}
    foreach ($fixture in $manifest.fixtures) {
        $destination = Join-Path $corpusDirectory $fixture.name
        if (-not (Test-Path -LiteralPath $destination)) {
            $url = "https://raw.githubusercontent.com/$($manifest.repository)/$($manifest.revision)/$($manifest.directory)/$($fixture.name)"
            Invoke-WebRequest -Uri $url -OutFile $destination
        }
        $before = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($before -ne $fixture.sha256 -or (Get-Item -LiteralPath $destination).Length -ne $fixture.bytes) {
            throw "Fixture integrity mismatch: $($fixture.name)"
        }
        $raw = & $executable inspect $destination
        if ($LASTEXITCODE -ne 0) { throw "Parser failed: $($fixture.name)" }
        $report = $raw | ConvertFrom-Json
        if ([string]::IsNullOrWhiteSpace($report.text)) { throw "No text: $($fixture.name)" }
        $reports[$fixture.name] = $report
        $after = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($before -ne $after) { throw 'Source was modified' }
    }
    # The DOC and DOCX fixture pair have matching content; this compares extraction
    # across two containers, not against an independent layout/text oracle.
    if ($reports['SampleDoc.doc'].text.Trim() -cne $reports['SampleDoc.docx'].text.Trim()) {
        throw 'Paired DOC/DOCX text differs'
    }
    $runDirectory = Join-Path $corpusDirectory ([guid]::NewGuid().ToString())
    New-Item -ItemType Directory -Path $runDirectory | Out-Null
    $output = Join-Path $runDirectory 'edited.docx'
    foreach ($legacyName in @('simple.doc', 'SampleDoc.doc')) {
        $legacySource = Join-Path $corpusDirectory $legacyName
        $raw = & $executable import-doc $legacySource
        if ($LASTEXITCODE -ne 0) { throw "DOC import failed: $legacyName" }
        $imported = $raw | ConvertFrom-Json
        if ($imported.compatibility.fidelity_verified) { throw 'Unexpected fidelity claim' }
        $modelPath = Join-Path $runDirectory "$legacyName.json"
        [IO.File]::WriteAllText($modelPath, ($imported.document | ConvertTo-Json -Depth 64), [Text.UTF8Encoding]::new($false))
        $convertedPath = Join-Path $runDirectory "$legacyName.docx"
        & $executable create-docx $modelPath $convertedPath | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "DOCX export failed: $legacyName" }
        $raw = & $executable inspect $convertedPath
        if ($LASTEXITCODE -ne 0) { throw "Converted DOCX reopen failed: $legacyName" }
        $converted = $raw | ConvertFrom-Json
        if ($converted.text.Trim() -cne $reports[$legacyName].text.Trim()) { throw "DOC conversion changed text: $legacyName" }
        $fixture = $manifest.fixtures | Where-Object name -eq $legacyName
        if ((Get-FileHash -LiteralPath $legacySource -Algorithm SHA256).Hash.ToLowerInvariant() -ne $fixture.sha256) { throw 'DOC source modified' }
    }
    $source = Join-Path $corpusDirectory 'SampleDoc.docx'
    $raw = & $executable replace $source $output 'test document' 'Rust 办公测试文档'
    if ($LASTEXITCODE -ne 0) { throw 'Real DOCX replacement failed' }
    $edit = $raw | ConvertFrom-Json
    if ($edit.replacements -ne 1 -or $edit.changed_parts.Count -ne 1 -or $edit.changed_parts[0] -ne 'word/document.xml') {
        throw 'Unexpected edited parts or replacement count'
    }
    $raw = & $executable inspect $output
    if ($LASTEXITCODE -ne 0) { throw 'Edited DOCX cannot be reopened' }
    $reopened = $raw | ConvertFrom-Json
    $expected = $reports['SampleDoc.docx'].text.Replace('test document', 'Rust 办公测试文档')
    if ($reopened.text -cne $expected) { throw 'Edited text differs from expected' }
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.fixtures[2].sha256) {
        throw 'Replacement modified source'
    }
    [pscustomobject]@{
        fixtureCount = $manifest.fixtures.Count
        pairedTextMatches = $true
        editedDocxReopens = $true
        legacyDocConversions = 2
        preservedParts = $edit.preserved_parts
        output = $output
        limitation = 'Text and package preservation only; no independent visual/layout verification.'
    } | ConvertTo-Json
} finally {
    Pop-Location
}
