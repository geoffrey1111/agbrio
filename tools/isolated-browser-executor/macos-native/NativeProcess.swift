// Native metadata and graceful actions for an exact owned Chromium process.
// Never reads argv, environment, credentials, browser pages or profile contents.
import Foundation
import AppKit
import CoreGraphics
import Darwin

enum Refusal: Error { case code(String) }
func refuse(_ code: String) throws -> Never { throw Refusal.code(code) }
func json(_ value: Any) throws -> String {
    String(data: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), encoding: .utf8)!
}
func plain(_ path: String) throws -> String {
    guard path.hasPrefix("/"), path.utf8.count <= 4096, !path.contains("\0") else { try refuse("SECURITY_PROFILE_INVALID") }
    let resolved = URL(fileURLWithPath: path).resolvingSymlinksInPath().standardizedFileURL.path
    guard resolved == path else { try refuse("SECURITY_PROFILE_LINK_REJECTED") }
    return resolved
}
func ownedJSON(_ path: String) throws -> [String: Any] {
    let fd = open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
    guard fd >= 0 else { try refuse("PROFILE_OWNERSHIP_UNPROVEN") }
    defer { close(fd) }
    var info = stat()
    guard fstat(fd, &info) == 0, (info.st_mode & S_IFMT) == S_IFREG,
          info.st_uid == geteuid(), info.st_nlink == 1, info.st_size > 0, info.st_size <= 8192,
          (info.st_mode & 0o077) == 0 else { try refuse("PROFILE_OWNERSHIP_UNPROVEN") }
    var bytes = [UInt8](repeating: 0, count: Int(info.st_size))
    let count = bytes.withUnsafeMutableBytes { read(fd, $0.baseAddress, $0.count) }
    guard count == bytes.count, let value = try JSONSerialization.jsonObject(with: Data(bytes)) as? [String: Any] else { try refuse("PROFILE_OWNERSHIP_UNPROVEN") }
    return value
}
func profile(_ value: Any?) throws -> String {
    guard let path = value as? String else { try refuse("SECURITY_PROFILE_INVALID") }
    let root = try plain(path)
    let marker = try ownedJSON(root + "/aiwr-profile-owner.json")
    guard marker["schema"] as? String == "AIWR_OWNED_CHROMIUM_PROFILE_V1",
          let identity = marker["ownerId"] as? String, UUID(uuidString: identity) != nil else { try refuse("PROFILE_OWNERSHIP_UNPROVEN") }
    return root
}
func singletonPID(_ root: String) throws -> Int32? {
    let file = root + "/SingletonLock"
    var info = stat()
    if lstat(file, &info) != 0 { if errno == ENOENT { return nil }; try refuse("PROFILE_IN_USE") }
    guard (info.st_mode & S_IFMT) == S_IFLNK, info.st_uid == geteuid() else { try refuse("PROFILE_IN_USE") }
    let target = try FileManager.default.destinationOfSymbolicLink(atPath: file)
    guard let separator = target.lastIndex(of: "-"), String(target[..<separator]) == ProcessInfo.processInfo.hostName,
          let pid = Int32(target[target.index(after: separator)...]), pid > 0 else { try refuse("PROFILE_IN_USE") }
    return pid
}
@MainActor func visible(_ pid: Int32) -> Bool {
    guard let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return false }
    return windows.contains { window in
        guard (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid,
              (window[kCGWindowLayer as String] as? NSNumber)?.intValue == 0,
              let bounds = window[kCGWindowBounds as String] as? [String: Any] else { return false }
        return ((bounds["Width"] as? NSNumber)?.doubleValue ?? 0) > 0 && ((bounds["Height"] as? NSNumber)?.doubleValue ?? 0) > 0
    }
}
@MainActor func inspect(_ pid: Int32) throws -> [String: Any]? {
    guard pid > 0 else { try refuse("SECURITY_BROWSER_IDENTITY_CHANGED") }
    var info = proc_bsdinfo()
    let count = withUnsafeMutablePointer(to: &info) { proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, $0, Int32(MemoryLayout<proc_bsdinfo>.size)) }
    if count == 0 {
        if kill(pid, 0) != 0 && errno == ESRCH { return nil }
        try refuse("SECURITY_BROWSER_IDENTITY_CHANGED")
    }
    guard Int(count) == MemoryLayout<proc_bsdinfo>.size, info.pbi_uid == geteuid() else { try refuse("SECURITY_BROWSER_IDENTITY_CHANGED") }
    var image = [CChar](repeating: 0, count: Int(PROC_PIDPATHINFO_MAXSIZE))
    let length = image.withUnsafeMutableBytes { proc_pidpath(pid, $0.baseAddress, UInt32($0.count)) }
    guard length > 0 else { try refuse("SECURITY_BROWSER_IDENTITY_CHANGED") }
    let binary = URL(fileURLWithPath: String(cString: image)).resolvingSymlinksInPath().path
    return ["pid": pid, "uid": geteuid(), "binary": binary,
            "started": "\(info.pbi_start_tvsec):\(info.pbi_start_tvusec)", "visible": visible(pid)]
}
@MainActor func verified(_ request: [String: Any], closing: Bool) throws -> (NSRunningApplication, Int32) {
    let root = try profile(request["profile"])
    guard let pid = request["pid"] as? Int32, let image = request["binary"] as? String, let started = request["started"] as? String,
          try singletonPID(root) == pid, let actual = try inspect(pid), actual["binary"] as? String == image,
          actual["started"] as? String == started else { try refuse("SECURITY_BROWSER_IDENTITY_CHANGED") }
    let lease = try ownedJSON(root + "/aiwr-browser-session.json")
    guard lease["schema"] as? String == "AIWR_BROWSER_SESSION_V1", lease["profile"] as? String == root,
          lease["pid"] as? Int32 == pid, lease["binary"] as? String == image, lease["started"] as? String == started,
          let mode = lease["mode"] as? String, closing ? mode == "HUMAN_ONLY" : ["AUTOMATED", "HUMAN_ONLY"].contains(mode),
          let app = NSRunningApplication(processIdentifier: pid) else { try refuse("SECURITY_BROWSER_IDENTITY_CHANGED") }
    return (app, pid)
}
func guardProfile(_ request: [String: Any]) throws {
    let root = try profile(request["profile"])
    let fd = open(root + "/.agbrio-security.lock", O_CREAT | O_RDWR | O_NOFOLLOW | O_CLOEXEC, 0o600)
    guard fd >= 0 else { try refuse("SECURITY_RECOVERY_GUARD_UNAVAILABLE") }
    defer { close(fd) }
    var info = stat()
    guard fstat(fd, &info) == 0, (info.st_mode & S_IFMT) == S_IFREG, info.st_uid == geteuid(),
          info.st_nlink == 1, (info.st_mode & 0o777) == 0o600 else { try refuse("SECURITY_RECOVERY_GUARD_UNAVAILABLE") }
    if flock(fd, LOCK_EX | LOCK_NB) != 0 { print("BUSY"); fflush(stdout); return }
    defer { _ = flock(fd, LOCK_UN) }
    print("ACQUIRED"); fflush(stdout)
    // EOF from the exact calling Node process releases the kernel lock even
    // when its task crashes. Keep the inode; never unlink a successor's lock.
    var byte: UInt8 = 0
    _ = read(STDIN_FILENO, &byte, 1)
}
@main @MainActor struct NativeBoundary {
    static func main() {
        do {
            // One bounded JSON line. The lock operation then waits for EOF.
            var bytes = Data()
            while let part = try FileHandle.standardInput.read(upToCount: 1), !part.isEmpty {
                if part[0] == 10 { break }
                bytes.append(part)
                if bytes.count > 8192 { try refuse("SECURITY_REQUEST_TOO_LARGE") }
            }
            guard let request = try JSONSerialization.jsonObject(with: bytes) as? [String: Any],
                  Set(request.keys).isSubset(of: ["operation", "profile", "pid", "binary", "started"]),
                  let operation = request["operation"] as? String else { try refuse("SECURITY_REQUEST_INVALID") }
            switch operation {
            case "guard": try guardProfile(request)
            case "inventory":
                let root = try profile(request["profile"])
                if let pid = try singletonPID(root), let item = try inspect(pid) { print(try json([item])) } else { print("[]") }
            case "close":
                let (app, _) = try verified(request, closing: true)
                guard app.terminate() else { try refuse("SECURITY_BROWSER_CLOSE_PENDING") }
                print(try json(["status": "CLOSE_REQUESTED"]))
            case "focus":
                let (app, pid) = try verified(request, closing: false)
                guard visible(pid) else { try refuse("SECURITY_BROWSER_WINDOW_UNAVAILABLE") }
                _ = app.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])
                let deadline = Date().addingTimeInterval(1)
                while Date() < deadline {
                    if NSWorkspace.shared.frontmostApplication?.processIdentifier == pid { print(try json(["status": "FOREGROUND"])); return }
                    RunLoop.current.run(until: Date().addingTimeInterval(0.05))
                }
                print(try json(["status": "ATTENTION"]))
            default: try refuse("SECURITY_REQUEST_UNSUPPORTED")
            }
        } catch Refusal.code(let code) { print("{\"errorCode\":\"\(code)\"}"); exit(1) }
        catch { print("{\"errorCode\":\"SECURITY_NATIVE_UNAVAILABLE\"}"); exit(1) }
    }
}
