# Read-only on-device proof for the exact running Camoufox process.
# Never fetch Name, Value, AutomationId, page URL, page title or profile files.
# The test target is an already opened browser; no remote debugger or test tab is created.
param(
    [ValidateRange(0, 2147483647)]
    [int]$BrowserProcessId = 0
)

$ErrorActionPreference = 'Stop'
try {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes

    if ($BrowserProcessId -eq 0) {
        $candidates = @(Get-Process -Name 'camoufox' -ErrorAction SilentlyContinue |
            Where-Object { $_.MainWindowHandle -ne [IntPtr]::Zero })
        if ($candidates.Count -ne 1) {
            throw 'BROWSER_PROCESS_AMBIGUOUS'
        }
        $BrowserProcessId = $candidates[0].Id
    }
    $browser = Get-Process -Id $BrowserProcessId -ErrorAction Stop
    if ($browser.ProcessName -ne 'camoufox') {
        throw 'BROWSER_PROCESS_INVALID'
    }

    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $pidCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ProcessIdProperty,
        $BrowserProcessId
    )
    $windows = $root.FindAll(
        [System.Windows.Automation.TreeScope]::Children,
        $pidCondition
    )
    $tabItems = 0
    $editControls = 0
    $windowCount = 0
    $tabCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::TabItem
    )
    $editCondition = [System.Windows.Automation.PropertyCondition]::new(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Edit
    )
    foreach ($window in $windows) {
        if ($window.Current.ControlType -ne [System.Windows.Automation.ControlType]::Window) {
            continue
        }
        $windowCount += 1
        $tabItems += $window.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            $tabCondition
        ).Count
        $editControls += $window.FindAll(
            [System.Windows.Automation.TreeScope]::Descendants,
            $editCondition
        ).Count
    }
    if ($windowCount -eq 0) {
        throw 'NO_VISIBLE_BROWSER_WINDOW'
    }
    if ($editControls -eq 0) {
        throw 'NO_ACCESSIBLE_NAVIGATION_CONTROL'
    }
    if ($tabItems -eq 0) {
        throw 'NO_ACCESSIBLE_BROWSER_TABS'
    }
    # No content, title, target path, URL, name or credential is accessed or emitted.
    Write-Output "WINDOWS_UIA_PASS=TRUE;WINDOWS=$windowCount;TAB_ITEMS=$tabItems;EDIT_CONTROLS=$editControls"
    exit 0
}
catch {
    # Never dump arbitrary .NET exception text. A closed error code is enough.
    Write-Output 'WINDOWS_UIA_PASS=FALSE;REASON=UNAVAILABLE_OR_UNSUPPORTED'
    exit 2
}
