param([string]$Helper=(Join-Path $PSScriptRoot '../scripts/open-application-menu.ps1'))
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
function Menu-Root([string]$Mode){
    $result=[pscustomobject]@{reads=0;mode=$Mode}
    $result|Add-Member ScriptMethod FindAll {
        param($scope,$condition)
        if($condition -ne [System.Windows.Automation.Condition]::TrueCondition){return}
        $this.reads++
        if($this.mode -eq 'changing-route' -and $this.reads -gt 2){return}
        $type=if($this.mode -eq 'inspect' -and $this.reads -ge 3){[System.Windows.Automation.ControlType]::Button}else{[System.Windows.Automation.ControlType]::MenuItem}
        $item=[pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';IsOffscreen=$false;IsEnabled=$true;ControlType=$type}}
        $item|Add-Member NoteProperty Cached $item.Current
        $item|Add-Member ScriptMethod GetCurrentPattern {
            param($pattern)
            $result=[pscustomobject]@{Current=[pscustomobject]@{ExpandCollapseState=[System.Windows.Automation.ExpandCollapseState]::Expanded}}
            $result|Add-Member ScriptMethod Expand {}
            $result
        }
        $item
    }
    $result
}
$failures=[Collections.Generic.List[string]]::new()
foreach($mode in @('inspect','expanded','changing-route')){
    try{
        $window=Menu-Root $mode
        if($mode -eq 'inspect'){
            $entry=& $Helper -Root $window -Name File -Inspect
            if($entry.Current.ControlType -ne [System.Windows.Automation.ControlType]::Button){throw 'Inspection returned a submenu instead of the later button'}
        }else{
            $entry=& $Helper -Root $window -Name File -PassThru
            if($entry.Current.ControlType -ne [System.Windows.Automation.ControlType]::MenuItem){throw 'The existing submenu route was not retained'}
        }
        $expected=if($mode -eq 'inspect'){3}else{1}
        if($window.reads -ne $expected){throw 'Menu discovery did not retain one tree snapshot per attempt'}
        Write-Output ('Application menu '+$mode+': passed')
    }catch{
        $failures.Add($mode+': '+$_.Exception.Message)
        Write-Output ('Application menu '+$mode+': failed')
    }
}
if($failures.Count){throw ($failures -join [Environment]::NewLine)}

$popup=[pscustomobject]@{reads=0;opened=$false;expanded=$false}
$overflow=[pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menus';Name='Menus';IsOffscreen=$false;IsEnabled=$true;ControlType=[System.Windows.Automation.ControlType]::Button};Root=$popup}
$overflow|Add-Member NoteProperty Cached $overflow.Current
$overflow|Add-Member ScriptMethod GetCurrentPattern {
    param($pattern)
    $invoke=[pscustomobject]@{Root=$this.Root}
    $invoke|Add-Member ScriptMethod Invoke {$this.Root.opened=$true}
    $invoke
}
$submenu=[pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';IsOffscreen=$false;IsEnabled=$true;ControlType=[System.Windows.Automation.ControlType]::MenuItem};Root=$popup}
$submenu|Add-Member NoteProperty Cached $submenu.Current
$submenu|Add-Member ScriptMethod GetCurrentPattern {
    param($pattern)
    $expand=[pscustomobject]@{Root=$this.Root;Current=[pscustomobject]@{ExpandCollapseState=[System.Windows.Automation.ExpandCollapseState]::Expanded}}
    $expand|Add-Member ScriptMethod Expand {$this.Root.expanded=$true}
    $expand
}
$popup|Add-Member NoteProperty Overflow $overflow
$popup|Add-Member NoteProperty Submenu $submenu
$popup|Add-Member ScriptMethod FindAll {
    param($scope,$condition)
    $this.reads++
    if(!$this.opened){return $this.Overflow}
    if($this.reads -eq 2){throw [System.Windows.Automation.ElementNotAvailableException]::new('Popup is materializing')}
    $this.Submenu
}
& $Helper -Root $popup -Name File
if($popup.reads -ne 3 -or !$popup.expanded){throw 'Transient popup query did not retry and expand the owned File submenu'}
Write-Output 'Application menu popup materialization: passed'
