#Requires -Version 5.1
<#
.SYNOPSIS
  Sequential dev pipeline: implement → de-sloppify → verify → (optional commit).

.DESCRIPTION
  Runs non-interactive `claude -p` steps in order. Each step gets a fresh context
  window and builds on filesystem state from the previous step.
  Pattern: autonomous-loops skill (Sequential Pipeline + De-Sloppify).

.PARAMETER Task
  Task description. If omitted, reads the first unchecked P0/P1 item from SHARED_TASK_NOTES.md.

.PARAMETER Commit
  Run a fourth step that creates a conventional commit. Off by default.

.EXAMPLE
  .\scripts\dev-pipeline.ps1
  .\scripts\dev-pipeline.ps1 -Task "Add storage unit tests for batch insert"
  .\scripts\dev-pipeline.ps1 -Commit
#>
param(
    [string]$Task = "",
    [switch]$Commit
)

# PowerShell equivalent of `set -e` — stop on first error
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $RepoRoot

$SharedNotes = Join-Path $RepoRoot "SHARED_TASK_NOTES.md"

function Get-TaskFromNotes {
    if (-not (Test-Path $SharedNotes)) {
        # SHARED_TASK_NOTES.md is an untracked internal planning doc — absent on clean clones.
        Write-Host "SHARED_TASK_NOTES.md not found (internal planning docs are not tracked in git)." -ForegroundColor Yellow
        Write-Host "Pass a task explicitly instead, e.g.:" -ForegroundColor Yellow
        Write-Host '  .\scripts\dev-pipeline.ps1 -Task "Add storage unit tests for batch insert"' -ForegroundColor Yellow
        Write-Host "See README.md for project context." -ForegroundColor Yellow
        exit 1
    }
    $lines = Get-Content $SharedNotes -Encoding UTF8
    foreach ($line in $lines) {
        if ($line -match '^\s*-\s*\[\s\]\s*\*\*(.+?)\*\*\s*—\s*(.+)$') {
            return "$($Matches[1]): $($Matches[2])"
        }
        if ($line -match '^\s*-\s*\[\s\]\s*(.+)$') {
            return $Matches[1].Trim()
        }
    }
    Write-Host "No unchecked task found in SHARED_TASK_NOTES.md — pass one with -Task `"<description>`"." -ForegroundColor Yellow
    exit 1
}

function Invoke-ClaudeStep {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Prompt
    )
    Write-Host ""
    Write-Host "=== Step: $Name ===" -ForegroundColor Cyan
    Write-Host $Prompt.Substring(0, [Math]::Min(120, $Prompt.Length)) -ForegroundColor DarkGray
    if ($Prompt.Length -gt 120) { Write-Host "..." -ForegroundColor DarkGray }

    & claude -p $Prompt
    if ($LASTEXITCODE -ne 0) {
        throw "Step '$Name' failed (exit code $LASTEXITCODE)."
    }
}

if ([string]::IsNullOrWhiteSpace($Task)) {
    $Task = Get-TaskFromNotes
    Write-Host "Task from SHARED_TASK_NOTES.md: $Task" -ForegroundColor Yellow
} else {
    Write-Host "Task from parameter: $Task" -ForegroundColor Yellow
}

# --- Step 1: Implement ---
# Fresh context; read specs and implement the task. Let the agent be thorough with tests.
# README.md is always tracked; internal planning docs are included only when present.
$contextDocs = @('README.md') + @(@('SHARED_TASK_NOTES.md', 'STATUS.md', 'PLAN.md') |
    Where-Object { Test-Path (Join-Path $RepoRoot $_) })
$implementPrompt = @"
You are working on AllTokens (Rust + React + Tauri token tracker) at $RepoRoot.

Read $($contextDocs -join ', ') for context.

Task: $Task

Implement this task. Match existing project conventions. Write tests where appropriate.
Do NOT create new documentation files unless the task explicitly requires it.
Do NOT commit unless explicitly told to in this prompt.
"@

Invoke-ClaudeStep -Name "Implement" -Prompt $implementPrompt

# --- Step 2: De-sloppify ---
# Separate cleanup pass — remove test/code slop without constraining the implementer.
$desloppifyPrompt = @"
Review all files changed in the working tree for AllTokens at $RepoRoot.

Remove:
- Tests that verify language/framework behavior rather than business logic
- Redundant type checks the type system already enforces
- Over-defensive error handling for impossible states
- console.log / dbg! noise, commented-out code, unnecessary comments

Keep all real business logic tests. Run the test suite after cleanup.
Do NOT add new features. Do NOT commit.
"@

Invoke-ClaudeStep -Name "De-sloppify" -Prompt $desloppifyPrompt

# --- Step 3: Verify ---
# Build, lint, typecheck, test — fix failures only, no new features.
$blockerTarget = if (Test-Path $SharedNotes) { 'the SHARED_TASK_NOTES.md Notes section' } else { 'your final output' }
$verifyPrompt = @"
At $RepoRoot (AllTokens), run the full verification suite and fix any failures:

1. cargo test --workspace
2. cargo build --release -p alltokens-cli
3. cd frontend && npm run typecheck && npm run lint && npm run test && npm run build

Do not add new features. Do not commit.
If a step cannot run (missing deps), document the blocker in $blockerTarget.
"@

Invoke-ClaudeStep -Name "Verify" -Prompt $verifyPrompt

# --- Step 4: Commit (optional) ---
if ($Commit) {
    $commitPrompt = @"
At $RepoRoot, create a conventional commit for all relevant changes.

Task was: $Task

Use a concise message focused on why (e.g. test: add storage batch insert tests).
Stage only files related to the task. Do not push.
"@
    Invoke-ClaudeStep -Name "Commit" -Prompt $commitPrompt
} else {
    Write-Host ""
    Write-Host "Skipping commit (use -Commit to enable)." -ForegroundColor DarkYellow
}

Write-Host ""
Write-Host "Pipeline complete." -ForegroundColor Green
