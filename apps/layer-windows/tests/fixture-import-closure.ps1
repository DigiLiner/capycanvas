param([string]$Scripts=(Join-Path $PSScriptRoot '../scripts'),[switch]$SelfTestOnly)
$ErrorActionPreference='Stop'
$script:closureAsts=@{}
function Read-ClosureAst($Path){
 $Path=[IO.Path]::GetFullPath($Path)
 if(!$script:closureAsts.ContainsKey($Path)){
  $tokens=$null;$errors=$null;$tree=[Management.Automation.Language.Parser]::ParseFile($Path,[ref]$tokens,[ref]$errors)
  if($errors.Count){throw ($errors|Out-String)};$script:closureAsts[$Path]=$tree
 }
 $script:closureAsts[$Path]
}
function Closure-Scope($Node){
 for($at=$Node;$at;$at=$at.Parent){
  if($at -is [Management.Automation.Language.ScriptBlockAst] -and (!$at.Parent -or $at.Parent -is [Management.Automation.Language.FunctionDefinitionAst])){return $at}
  if($at -is [Management.Automation.Language.ScriptBlockAst] -and $at.Parent -is [Management.Automation.Language.ScriptBlockExpressionAst]){
   $command=$at.Parent.Parent
   if($command -is [Management.Automation.Language.CommandAst] -and $command.InvocationOperator -eq 'Ampersand'){return $at}
  }
 }
}
function Closure-SameScope($Node,$Scope){[object]::ReferenceEquals((Closure-Scope $Node),$Scope)}
function Closure-Strings($Node){
 if(@($Node.FindAll({param($n)$n -is [Management.Automation.Language.VariableExpressionAst]},$true)).Count){throw 'Nonliteral imported function list'}
 @($Node.FindAll({param($n)$n -is [Management.Automation.Language.StringConstantExpressionAst]},$true)|ForEach-Object Value)
}
function Closure-Path($Node,$Directory){
 $join=$Node.Find({param($n)$n -is [Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'Join-Path'},$true)
 if(!$join){return $null}
 $last=$join.CommandElements[-1]
 if($last -isnot [Management.Automation.Language.StringConstantExpressionAst]){return $null}
 $name=$last.Value
 if($name.StartsWith('tools/')){return [IO.Path]::GetFullPath((Join-Path (Join-Path $Directory '../../..') $name))}
 [IO.Path]::GetFullPath((Join-Path $Directory $name))
}
function Closure-Assignment($Node){for($at=$Node.Parent;$at;$at=$at.Parent){if($at -is [Management.Automation.Language.AssignmentStatementAst]){return $at}}}
function Closure-Names($Selector){
 $test=$Selector.Arguments[0]
 $named=$test.Find({param($n)$n -is [Management.Automation.Language.BinaryExpressionAst] -and $n.Operator -in @('Ieq','Ceq') -and $n.Left -is [Management.Automation.Language.MemberExpressionAst] -and $n.Left.Member.Value -eq 'Name'},$true)
 if(!$named){throw "Unsupported FUNCTION import selector: $($test.Extent.Text)"}
 if($named.Right -is [Management.Automation.Language.StringConstantExpressionAst]){return @($named.Right.Value)}
 if($named.Right -is [Management.Automation.Language.VariableExpressionAst]){
  $variable=$named.Right.VariablePath.UserPath
  for($at=$Selector.Parent;$at;$at=$at.Parent){
   if($at -is [Management.Automation.Language.ForEachStatementAst] -and $at.Variable.VariablePath.UserPath -eq $variable){return @(Closure-Strings $at.Condition)}
  }
 }
 throw "Unsupported FUNCTION import names: $($test.Extent.Text)"
}
function Closure-Branch($Node){
 for($at=$Node;$at.Parent;$at=$at.Parent){if($at -is [Management.Automation.Language.StatementBlockAst] -and $at.Parent -is [Management.Automation.Language.IfStatementAst]){return @{owner=$at.Parent;block=$at}}}
}
function Get-ClosureImports($Tree,$Directory){
 $imports=[Collections.Generic.List[object]]::new()
 $evaluators=@($Tree.FindAll({param($n)($n -is [Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'Invoke-Expression') -or ($n -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Member.Value -eq 'Create' -and $n.Expression.Extent.Text -match '(?i)scriptblock')},$true))
 $parsers=@($Tree.FindAll({param($n)$n -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Member.Value -in @('ParseFile','ParseInput')},$true))
 foreach($parser in $parsers){
  $binding=Closure-Assignment $parser;$scope=Closure-Scope $parser
  if(!$binding -or $binding.Left -isnot [Management.Automation.Language.VariableExpressionAst]){throw 'Unbound AST parser result'}
  $catalog=$binding.Left.VariablePath.UserPath;$input=$parser.Arguments[0]
  if($parser.Member.Value -eq 'ParseInput'){
   if($input -isnot [Management.Automation.Language.VariableExpressionAst]){throw 'Unsupported ParseInput provider'}
   $inputName=$input.VariablePath.UserPath
   $read=$scope.Find({param($n)$n -is [Management.Automation.Language.AssignmentStatementAst] -and $n.Left.Extent.Text -eq ('$'+$inputName) -and $n.Extent.StartOffset -lt $parser.Extent.StartOffset},$true)
   if(!$read){throw 'Missing ParseInput source binding'};$input=$read.Right
  }
  $provider=Closure-Path $input $Directory
  if(!$provider){
   if($scope.Parent -isnot [Management.Automation.Language.FunctionDefinitionAst]){throw 'Unsupported dynamic FUNCTION provider'}
   $loader=$scope.Parent.Name
   $calls=@($Tree.FindAll({param($n)$n -is [Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq $loader},$true))
   if(!$calls.Count){throw "No explicit calls to AST import loader $loader"}
   foreach($call in $calls){
    $fileNames=@(Closure-Strings $call.CommandElements[1]);$names=@(Closure-Strings $call.CommandElements[2])
    if($fileNames.Count -ne 1 -or !$names.Count){throw 'Nonliteral AST import-loader call'}
    $imports.Add(@{source=(Join-Path $Directory $fileNames[0]);names=$names;scope=(Closure-Scope $call);branch=(Closure-Branch $call);site=$call})
   }
   continue
  }
  $selectors=@($scope.FindAll({param($n)$n -is [Management.Automation.Language.InvokeMemberExpressionAst] -and $n.Member.Value -in @('Find','FindAll') -and $n.Expression -is [Management.Automation.Language.VariableExpressionAst] -and $n.Expression.VariablePath.UserPath -eq $catalog -and $n.Extent.StartOffset -gt $parser.Extent.StartOffset -and (Closure-SameScope $n $scope)},$true))
  foreach($selector in $selectors){
   if(!$selector.Arguments[0].Extent.Text.Contains('FunctionDefinitionAst')){continue}
   $selection=Closure-Assignment $selector;if(!$selection){throw 'Unbound FUNCTION selection'};$selected=$selection.Left.Extent.Text.TrimStart('$')
   $references=@($selected)
   foreach($alias in $scope.FindAll({param($n)$n -is [Management.Automation.Language.AssignmentStatementAst]},$true)){
    if($alias.Right.Extent.Text.Contains('$'+$selected) -and $alias.Right.Extent.Text.Contains('.Extent.Text')){$references+=,$alias.Left.Extent.Text.TrimStart('$')}
   }
   $used=@($evaluators|Where-Object {$evaluation=$_;(Closure-SameScope $evaluation $scope) -and @($evaluation.FindAll({param($n)$n -is [Management.Automation.Language.VariableExpressionAst] -and $n.VariablePath.UserPath -in $references},$true)).Count})
   if(!$used.Count){continue}
   $names=@(Closure-Names $selector);$imports.Add(@{source=$provider;names=$names;scope=$scope;branch=(Closure-Branch $selector);site=$selector})
  }
 }
 $imports.ToArray()
}
function Add-ClosureDefinitions($Scope,$Directory,$Available,$Mocks,$Seen){
 foreach($definition in $Scope.FindAll({param($n)$n -is [Management.Automation.Language.FunctionDefinitionAst]},$true)){
  if(Closure-SameScope $definition $Scope){$Available[$definition.Name -replace '^(script|global):','']=$definition}
 }
 foreach($write in $Scope.FindAll({param($n)$n -is [Management.Automation.Language.CommandAst] -and $n.GetCommandName() -eq 'Set-Content'},$true)){
  if(!(Closure-SameScope $write $Scope) -or $write.CommandElements.Count -ne 3){continue}
  $path=Closure-Path $write.CommandElements[1] $Directory
  if($path -and $path.EndsWith('.ps1') -and $write.CommandElements[2] -is [Management.Automation.Language.StringConstantExpressionAst]){
   $tokens=$null;$errors=$null;$mock=[Management.Automation.Language.Parser]::ParseInput($write.CommandElements[2].Value,[ref]$tokens,[ref]$errors)
   if($errors.Count){throw 'Invalid declared AST-import mock source'};$Mocks[[IO.Path]::GetFileName($path)]=$mock
  }
 }
 foreach($command in $Scope.FindAll({param($n)$n -is [Management.Automation.Language.CommandAst] -and $n.InvocationOperator -eq 'Dot'},$true)){
  if(!(Closure-SameScope $command $Scope)){continue}
  if($command.Extent.Text -match '(?i)scriptblock\]::Create'){continue}
  $path=Closure-Path $command.CommandElements[0] $Directory
  if(!$path){throw "Unsupported dot-source closure: $($command.Extent.Text)"}
  if($Seen.ContainsKey($path)){continue};$Seen[$path]=$true
  $tree=if($Mocks.ContainsKey([IO.Path]::GetFileName($path))){$Mocks[[IO.Path]::GetFileName($path)]}else{Read-ClosureAst $path}
  Add-ClosureDefinitions $tree ([IO.Path]::GetDirectoryName($path)) $Available $Mocks $Seen
 }
}
function Test-ClosureImport($Import,$Peers,$Directory,$Consumer){
 $available=@{};$mocks=@{};$seen=@{}
 if($Consumer){Add-ClosureDefinitions $Consumer $Directory $available $mocks $seen}
 $scopes=@();for($scope=$Import.scope;$scope;$scope=Closure-Scope $scope.Parent){$scopes=@($scope)+$scopes}
 foreach($scope in $scopes){Add-ClosureDefinitions $scope $Directory $available $mocks $seen}
 foreach($peer in $Peers){
  if(![object]::ReferenceEquals($peer.scope,$Import.scope)){continue}
  if($peer.branch -and !$Import.branch){continue}
  if($peer.branch -and $Import.branch -and [object]::ReferenceEquals($peer.branch.owner,$Import.branch.owner) -and ![object]::ReferenceEquals($peer.branch.block,$Import.branch.block)){continue}
  $provider=Read-ClosureAst $peer.source
  foreach($name in $peer.names){
   $definitions=@($provider.FindAll({param($n)$n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name},$true))
   if($definitions.Count -ne 1){throw "Expected one imported $name in $($peer.source)"};$available[$name]=$definitions[0]
  }
 }
 foreach($imported in $Import.names){
  $item=@{name=$imported;node=$available[$imported]};$body=$item.node.Body
  Add-ClosureDefinitions $body $Directory $available $mocks $seen
  foreach($command in $body.FindAll({param($n)$n -is [Management.Automation.Language.CommandAst]},$true)){
   $name=$command.GetCommandName()
   if(!$name){
    if($command.InvocationOperator -eq 'Dot'){continue}
    $path=Closure-Path $command.CommandElements[0] $Directory
    if($path -and (Test-Path -LiteralPath $path)){continue}
    $variable=$command.CommandElements[0]
    $parameters=@($item.node.Parameters)+@($body.ParamBlock.Parameters)
    if($variable -is [Management.Automation.Language.VariableExpressionAst] -and @($parameters|Where-Object {$_.Name.VariablePath.UserPath -eq $variable.VariablePath.UserPath -and $_.StaticType -eq [scriptblock]}).Count){continue}
    throw "Unresolved dynamic command in imported $($item.name): $($command.Extent.Text)"
   }
   if($available.ContainsKey($name)){continue}
   $builtin=Get-Command $name -CommandType Cmdlet,Alias -ErrorAction SilentlyContinue
   if($builtin){continue}
   throw "Missing imported function dependency: $([IO.Path]::GetFileName($Import.site.Extent.File)):$($Import.site.Extent.StartLineNumber) $($item.name) -> $name"
  }
 }
}
function Test-FixtureImportClosures($Directory){
 $all=@(Get-ChildItem -LiteralPath $Directory -Filter '*.ps1' -File);$sites=0;$failures=[Collections.Generic.List[string]]::new()
 foreach($file in $all){
  try{
  $tree=Read-ClosureAst $file.FullName;$imports=@(Get-ClosureImports $tree $Directory)
  foreach($import in $imports){
   $consumers=@($all|Where-Object {$_.FullName -ne $file.FullName}|ForEach-Object {
    $candidate=Read-ClosureAst $_.FullName
    $dots=@($candidate.FindAll({param($n)$n -is [Management.Automation.Language.CommandAst] -and $n.InvocationOperator -eq 'Dot'},$true)|Where-Object {(Closure-Path $_.CommandElements[0] $Directory) -eq $file.FullName})
    if($dots.Count){$candidate}
   })
   if(!$consumers.Count){Test-ClosureImport $import $imports $Directory $null}else{foreach($consumer in $consumers){Test-ClosureImport $import $imports $Directory $consumer}}
   $sites++
  }
  }catch{$failures.Add("$($file.Name): $($_.Exception.Message)")}
 }
 if($failures.Count){throw ($failures -join "`n")}
 "AST-imported function closure passed $sites actual import sites"
}
function Test-ClosureControls{
 $directory=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'closure-controls'))
 $provider=Join-Path $directory 'provider.ps1';$consumer=Join-Path $directory 'consumer.ps1'
 $tokens=$null;$errors=$null
 $script:closureAsts[$provider]=[Management.Automation.Language.Parser]::ParseInput('function Entry {Close-Preferences};function Close-Preferences {}',$provider,[ref]$tokens,[ref]$errors)
 $prefix='$ast=[Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot ''provider.ps1''),[ref]$null,[ref]$null);'
 $selector='foreach($name in @({0})){{$fn=$ast.Find({{param($n)$n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name}},$true);Invoke-Expression $fn.Extent.Text}}'
 foreach($case in @(
  @{name='complete imported closure';names="'Entry','Close-Preferences'";local='';missing=$false},
  @{name='missing Close-Preferences';names="'Entry'";local='';missing=$true},
  @{name='consumer-local definition';names="'Entry'";local='function Close-Preferences {}';missing=$false},
  @{name='independent child scope cannot mask';names="'Entry'";local='& {function Close-Preferences {}}';missing=$true}
 )){
  $tree=[Management.Automation.Language.Parser]::ParseInput(($prefix+($selector -f $case.names)+$case.local),$consumer,[ref]$tokens,[ref]$errors)
  if($errors.Count){throw ($errors|Out-String)}
  $imports=@(Get-ClosureImports $tree $directory);$failure=$null
  try{foreach($import in $imports){Test-ClosureImport $import $imports $directory $null}}catch{$failure=$_.Exception.Message}
  if($case.missing){if(!$failure -or !$failure.Contains('Entry -> Close-Preferences')){throw "Closure control did not detect $($case.name): $failure"}}
  elseif($failure){throw "Closure control rejected $($case.name): $failure"}
 }
 'AST-imported function closure controls passed 4 cases'
}
function Test-ActualImportControls($Directory){
 foreach($case in @(
  @{file='exercise-touch-gestures.ps1';before="'Close-Preferences',";after='';expected='Set-PinchPreferences -> Close-Preferences'},
  @{file='exercise-tool-features.ps1';before="@('Box','Tile'";after="@('Control','Box','Tile'";expected='Expected one imported Control'}
 )){
  $path=Join-Path $Directory $case.file;$source=[IO.File]::ReadAllText($path)
  $tokens=$null;$errors=$null;$tree=[Management.Automation.Language.Parser]::ParseInput($source.Replace($case.before,$case.after),$path,[ref]$tokens,[ref]$errors)
  if($errors.Count){throw ($errors|Out-String)}
  $imports=@(Get-ClosureImports $tree $Directory);$failure=$null
  try{foreach($import in $imports){Test-ClosureImport $import $imports $Directory $null}}catch{$failure=$_.Exception.Message}
  if(!$failure -or !$failure.Contains($case.expected)){throw "Actual import negative did not detect $($case.file): $failure"}
 }
 'Actual imported-function negatives passed Touch and feature helpers'
}
Test-ClosureControls
if(!$SelfTestOnly){Test-ActualImportControls ([IO.Path]::GetFullPath($Scripts))}
if(!$SelfTestOnly){Test-FixtureImportClosures ([IO.Path]::GetFullPath($Scripts))}
