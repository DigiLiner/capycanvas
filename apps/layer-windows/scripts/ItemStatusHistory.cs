using System;
using System.Collections.Generic;
using System.Windows.Automation;

public sealed class CapyItemStatusHistory : IDisposable {
    public sealed class Entry {
        public string Status;
        public long Timestamp;
    }
    readonly List<Entry> entries = new List<Entry>();
    AutomationElement element;
    public void Record(object sender, AutomationPropertyChangedEventArgs change) {
        if (change.Property != AutomationElement.ItemStatusProperty) return;
        lock (entries) entries.Add(new Entry {Status = change.NewValue as string, Timestamp = System.Diagnostics.Stopwatch.GetTimestamp()});
    }
    public void Watch(AutomationElement target) {
        element = target;
        Automation.AddAutomationPropertyChangedEventHandler(element, TreeScope.Element, Record, AutomationElement.ItemStatusProperty);
    }
    public Entry[] Snapshot() {lock (entries) return entries.ToArray();}
    public void Dispose() {
        if (element == null) return;
        Automation.RemoveAutomationPropertyChangedEventHandler(element, Record);
        element = null;
    }
}
