param([string]$Helper=(Join-Path $PSScriptRoot '../scripts/open-application-menu.ps1'))
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
function Menu-Root([string]$Mode){
    $result=[pscustomobject]@{reads=0;mode=$Mode}
    $result|Add-Member ScriptMethod FindAll {
        param($scope,$condition)
        if($condition.Value -ne 'application-menu-file'){return}
        $this.reads++
        if($this.mode -eq 'changing-route' -and $this.reads -gt 2){return}
        $type=if($this.mode -eq 'inspect' -and $this.reads -ge 3){[System.Windows.Automation.ControlType]::Button}else{[System.Windows.Automation.ControlType]::MenuItem}
        $item=[pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menu-file';Name='File';IsOffscreen=$false;IsEnabled=$true;ControlType=$type}}
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
        Write-Output ('Application menu '+$mode+': passed')
    }catch{
        $failures.Add($mode+': '+$_.Exception.Message)
        Write-Output ('Application menu '+$mode+': failed')
    }
}
if($failures.Count){throw ($failures -join [Environment]::NewLine)}
