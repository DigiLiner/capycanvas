param([string]$Helper=(Join-Path $PSScriptRoot '../scripts/open-application-menu.ps1'))
$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes
function Menu-Root([string]$Mode){
    $result=[pscustomobject]@{reads=0;mode=$Mode}
    $result|Add-Member ScriptMethod FindAll {
        param($scope,$condition)
        if(![object]::ReferenceEquals([System.Windows.Automation.CacheRequest]::Current.TreeFilter,[System.Windows.Automation.Automation]::RawViewCondition)){throw 'Menu snapshot cache did not retain the raw search view'}
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

$popup=[pscustomobject]@{full_reads=0;child_reads=0;popup_reads=0;opened=$false;expanded=$false;failure=$null}
$overflow=[pscustomobject]@{Current=[pscustomobject]@{AutomationId='application-menus';Name='Menus';IsOffscreen=$false;IsEnabled=$true;ControlType=[System.Windows.Automation.ControlType]::Button};Root=$popup}
$overflow|Add-Member NoteProperty Cached $overflow.Current
$overflow|Add-Member ScriptMethod GetCurrentPattern {
    param($pattern)
    $invoke=[pscustomobject]@{Root=$this.Root}
    $invoke|Add-Member ScriptMethod Invoke {$this.Root.opened=$true}
    $invoke
}
$submenu=[pscustomobject]@{Current=[pscustomobject]@{AutomationId='menu-file';Name='File';IsOffscreen=$false;IsEnabled=$true;ControlType=[System.Windows.Automation.ControlType]::MenuItem};Root=$popup}
$submenu|Add-Member NoteProperty Cached $submenu.Current
$submenu|Add-Member ScriptMethod GetCurrentPattern {
    param($pattern)
    $expand=[pscustomobject]@{Root=$this.Root;Current=[pscustomobject]@{ExpandCollapseState=[System.Windows.Automation.ExpandCollapseState]::Expanded}}
    $expand|Add-Member ScriptMethod Expand {$this.Root.expanded=$true}
    $expand
}
$popup|Add-Member NoteProperty Overflow $overflow
$popup|Add-Member NoteProperty Submenu $submenu
$popupHost=[pscustomobject]@{Root=$popup;Cached=[pscustomobject]@{Name='PopupHost';ClassName='Microsoft.UI.Content.PopupWindowSiteBridge';ControlType=[System.Windows.Automation.ControlType]::Pane}}
$popupHost|Add-Member ScriptMethod FindAll {
    param($scope,$condition)
    if($scope -ne [System.Windows.Automation.TreeScope]::Descendants -or $condition -ne [System.Windows.Automation.Condition]::TrueCondition){throw 'Unexpected popup search scope'}
    if(![object]::ReferenceEquals([System.Windows.Automation.CacheRequest]::Current.TreeFilter,[System.Windows.Automation.Automation]::RawViewCondition)){throw 'Popup snapshot cache did not retain the raw search view'}
    $this.Root.popup_reads++
    if($this.Root.failure){throw $this.Root.failure}
    if($this.Root.popup_reads -eq 1){throw [System.Windows.Automation.ElementNotAvailableException]::new('Popup host expired')}
    $this.Root.Submenu
}
$popup|Add-Member NoteProperty PopupHost $popupHost
$popup|Add-Member ScriptMethod FindAll {
    param($scope,$condition)
    if(![object]::ReferenceEquals([System.Windows.Automation.CacheRequest]::Current.TreeFilter,[System.Windows.Automation.Automation]::RawViewCondition)){throw 'Popup snapshot cache did not retain the raw search view'}
    if($condition -ne [System.Windows.Automation.Condition]::TrueCondition){throw 'Unexpected menu search condition'}
    if($scope -eq [System.Windows.Automation.TreeScope]::Descendants){
        $this.full_reads++
        if($this.opened){throw 'Compact lookup traversed the expensive background subtree'}
        return $this.Overflow
    }
    if($scope -ne [System.Windows.Automation.TreeScope]::Children -or !$this.opened){throw 'Unexpected compact search scope'}
    $this.child_reads++
    if($this.child_reads -eq 1){throw [System.Windows.Automation.ElementNotAvailableException]::new('Popup is materializing')}
    if($this.child_reads -eq 2){return}
    $this.PopupHost
}
& $Helper -Root $popup -Name File
if($popup.full_reads -ne 1 -or $popup.child_reads -ne 4 -or $popup.popup_reads -ne 2 -or !$popup.expanded){throw 'Transient popup query did not retain its bounded scope and expand the owned File submenu'}
Write-Output 'Application menu popup materialization: passed'
$popup.full_reads=0;$popup.child_reads=0;$popup.popup_reads=0;$popup.opened=$false;$popup.expanded=$false;$popup.failure='fixture-popup-provider-failure'
$failed=$false
try{& $Helper -Root $popup -Name File}catch{if($_.Exception.Message -notmatch 'fixture-popup-provider-failure'){throw};$failed=$true}
if(!$failed -or $popup.full_reads -ne 1 -or $popup.child_reads -ne 3 -or $popup.popup_reads -ne 1 -or $popup.expanded){throw 'Unexpected popup provider failure was suppressed'}
Write-Output 'Application menu popup provider failure: passed'
