// Helper processes left behind by an earlier run of this app.
//
// The app runs mediaremote-adapter as a long-lived child
// (Weft/NowPlaying.swift). A normal quit stops it, but when the app is
// killed outright or crashes, the child is handed to launchd and keeps
// running, one more at each launch. At the next launch the app finds them
// by their parent (launchd, pid 1), their user and the exact script path
// of its own bundle, so that two copies of the app at different paths
// (the published one, a development build) never stop each other's.
//
// Read with libproc and sysctl, not by running ps: no child process and
// no parsing of a tool's output.

import Darwin
import Foundation

public enum OrphanedHelpers {
    /// Whether a process with these arguments runs `script`: as an
    /// interpreter is given it, right after the interpreter's own path.
    /// Exact path only, so another copy of the app is never a match.
    public static func runs(_ script: String, arguments: [String]) -> Bool {
        arguments.count >= 2 && arguments[1] == script
    }

    /// The arguments in a `KERN_PROCARGS2` buffer: the argument count, the
    /// executable's path, NUL padding, then the arguments, each ended by a
    /// NUL (the environment follows). `nil` when it does not read so.
    public static func arguments(procArgs buffer: [UInt8]) -> [String]? {
        let countSize = MemoryLayout<Int32>.size
        guard buffer.count >= countSize else { return nil }
        let count = buffer.withUnsafeBytes { Int($0.loadUnaligned(as: Int32.self)) }
        guard count >= 0 else { return nil }
        var index = countSize
        // The executable's path, then its padding.
        while index < buffer.count, buffer[index] != 0 { index += 1 }
        while index < buffer.count, buffer[index] == 0 { index += 1 }
        var arguments: [String] = []
        while arguments.count < count {
            guard index < buffer.count, let end = buffer[index...].firstIndex(of: 0) else { return nil }
            arguments.append(String(decoding: buffer[index..<end], as: UTF8.self))
            index = end + 1
        }
        return arguments
    }

    /// The arguments of a running process of this user, if it can be read.
    public static func arguments(of pid: pid_t) -> [String]? {
        var mib: [Int32] = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > 0 else { return nil }
        var buffer = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buffer, &size, nil, 0) == 0 else { return nil }
        return arguments(procArgs: Array(buffer.prefix(size)))
    }

    /// Processes of this user that launchd adopted and that run `script`.
    public static func find(running script: String) -> [pid_t] {
        allProcesses().filter { isOrphan($0, running: script) }
    }

    /// Stops them: SIGTERM, then SIGKILL for those still there after
    /// `grace`. Returns how many there were. Blocks up to `grace`: call it
    /// off the main thread.
    @discardableResult
    public static func stop(running script: String, grace: TimeInterval = 2) -> Int {
        let found = find(running: script)
        guard !found.isEmpty else { return 0 }
        for pid in found { kill(pid, SIGTERM) }
        let deadline = Date.now.addingTimeInterval(grace)
        // Checked again before each signal: a pid freed meanwhile may
        // already be someone else's.
        var left = found
        while true {
            left = left.filter { isOrphan($0, running: script) }
            if left.isEmpty || Date.now >= deadline { break }
            Thread.sleep(forTimeInterval: 0.05)
        }
        for pid in left { kill(pid, SIGKILL) }
        return found.count
    }

    private static func isOrphan(_ pid: pid_t, running script: String) -> Bool {
        guard let info = bsdInfo(pid), info.pbi_ppid == 1, info.pbi_uid == getuid(),
              let arguments = arguments(of: pid) else { return false }
        return runs(script, arguments: arguments)
    }

    private static func bsdInfo(_ pid: pid_t) -> proc_bsdinfo? {
        var info = proc_bsdinfo()
        let size = Int32(MemoryLayout<proc_bsdinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, size) == size else { return nil }
        return info
    }

    private static func allProcesses() -> [pid_t] {
        let estimate = proc_listallpids(nil, 0)
        guard estimate > 0 else { return [] }
        // Room for the ones started since the estimate.
        var pids = [pid_t](repeating: 0, count: Int(estimate) + 64)
        let count = proc_listallpids(&pids, Int32(pids.count * MemoryLayout<pid_t>.size))
        guard count > 0 else { return [] }
        return pids.prefix(Int(count)).filter { $0 > 1 }
    }
}
