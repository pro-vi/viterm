// Shows whether macOS marks a held key's auto-repeats as repeats on this
// keyboard path (keyboard firmware, Karabiner, macOS), which decides whether
// the engine can tell a held shortcut from separate presses.
//
// Opens one window. Each key-down prints its key code, modifiers and AppKit's
// isARepeat flag, and each key-up prints its key code; characters are never
// printed. Closing the window, or 30 seconds, ends it with a count.
//
// Build: swiftc -O fork/key-repeat-probe/main.swift -o target/key-repeat-probe
import AppKit

final class ProbeView: NSView {
    var downs = 0
    var repeats = 0

    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        downs += 1
        if event.isARepeat { repeats += 1 }
        print(String(format: "%.3f down key=%d mods=%@ repeat=%@",
                     event.timestamp, event.keyCode, modifierNames(event.modifierFlags),
                     event.isARepeat ? "yes" : "no"))
        fflush(stdout)
    }

    override func keyUp(with event: NSEvent) {
        print(String(format: "%.3f up   key=%d", event.timestamp, event.keyCode))
        fflush(stdout)
    }

    func modifierNames(_ flags: NSEvent.ModifierFlags) -> String {
        var names: [String] = []
        if flags.contains(.command) { names.append("Cmd") }
        if flags.contains(.option) { names.append("Alt") }
        if flags.contains(.control) { names.append("Ctrl") }
        if flags.contains(.shift) { names.append("Shift") }
        return names.isEmpty ? "-" : names.joined(separator: "+")
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 520, height: 120),
                      styleMask: [.titled, .closable], backing: .buffered, defer: false)
window.title = "Key repeat probe: hold Alt+Left 1 s, then tap it 3 times, then close"
let view = ProbeView(frame: window.contentView!.bounds)
window.contentView = view
window.makeFirstResponder(view)
window.center()
window.makeKeyAndOrderFront(nil)
app.activate(ignoringOtherApps: true)

func finish() {
    print("\(view.downs) key-downs, \(view.repeats) marked repeat")
    fflush(stdout)
    exit(0)
}

// --self-check sends the window one synthetic Alt+Left press and one marked as
// a repeat, so a run that reports no repeats can be told apart from a probe
// that cannot see the flag.
if CommandLine.arguments.contains("--self-check") {
    for isRepeat in [false, true] {
        let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .option,
                                     timestamp: ProcessInfo.processInfo.systemUptime,
                                     windowNumber: window.windowNumber, context: nil,
                                     characters: "", charactersIgnoringModifiers: "",
                                     isARepeat: isRepeat, keyCode: 123)!
        window.sendEvent(event)
    }
    finish()
}
NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification,
                                       object: window, queue: nil) { _ in finish() }
Timer.scheduledTimer(withTimeInterval: 30, repeats: false) { _ in finish() }
app.run()
