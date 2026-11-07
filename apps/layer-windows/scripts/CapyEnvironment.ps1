if(!('CapyEnvironment' -as [type])){
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class CapyEnvironment {
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]
    public static extern bool SetEnvironmentVariableW(string name,string value);
}
'@
}
function Restore-CapyEnvironmentValue([string]$Name,$Value){
    $restored=if($null -eq $Value){[CapyEnvironment]::SetEnvironmentVariableW($Name,[NullString]::Value)}else{[CapyEnvironment]::SetEnvironmentVariableW($Name,$Value)}
    if(!$restored){throw "Could not restore environment variable $Name"}
}
