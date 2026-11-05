// Observe-only resource producer running as its owner. No process control, credentials,
// shell, provider calls or system-wide service configuration.
import Foundation
import Darwin
import CryptoKit

enum Refusal: Error { case code(String) }
func fail(_ code: String) throws -> Never { throw Refusal.code(code) }
func object(_ file: String) throws -> [String: Any] {
    let fd = open(file, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
    guard fd >= 0 else { try fail("RESOURCE_MONITOR_STORAGE_UNAVAILABLE") }
    defer { close(fd) }
    var info = stat()
    guard fstat(fd, &info) == 0, (info.st_mode & S_IFMT) == S_IFREG, info.st_uid == geteuid(), info.st_nlink == 1,
          (info.st_mode & 0o077) == 0, info.st_size > 0, info.st_size <= 65536 else { try fail("RESOURCE_MONITOR_STORAGE_UNTRUSTED") }
    var data = [UInt8](repeating: 0, count: Int(info.st_size))
    let count = data.withUnsafeMutableBytes { read(fd, $0.baseAddress, $0.count) }
    guard count == data.count, let value = try JSONSerialization.jsonObject(with: Data(data)) as? [String: Any] else { try fail("RESOURCE_MONITOR_STORAGE_INVALID") }
    return value
}
func trustedRoot(_ root: String) throws -> [String: Any]? {
    guard root.hasPrefix("/"), root.utf8.count <= 4096, !root.contains("\0"), URL(fileURLWithPath: root).resolvingSymlinksInPath().standardizedFileURL.path == root else { try fail("RESOURCE_MONITOR_PATH_INVALID") }
    var info = stat()
    if lstat(root, &info) != 0 { if errno == ENOENT { return nil }; try fail("RESOURCE_MONITOR_STORAGE_UNAVAILABLE") }
    guard (info.st_mode & S_IFMT) == S_IFDIR, info.st_uid == geteuid(), (info.st_mode & 0o777) == 0o700 else { try fail("RESOURCE_MONITOR_STORAGE_UNTRUSTED") }
    if !FileManager.default.fileExists(atPath: root + "/owner.json"), try FileManager.default.contentsOfDirectory(atPath: root).isEmpty { return nil }
    let marker = try object(root + "/owner.json")
    guard marker["schema"] as? String == "AGBRIO_MAC_RESOURCE_OWNER_V1", marker["ownerUid"] as? UInt32 == geteuid(),
          let host = marker["hostId"] as? String, UUID(uuidString: host) != nil else { try fail("RESOURCE_MONITOR_STORAGE_UNTRUSTED") }
    return marker
}
func processIdentity(_ pid: Int32) throws -> [String: Any] {
    guard pid > 0 else { try fail("RESOURCE_MONITOR_IDENTITY_UNAVAILABLE") }
    var info = proc_bsdinfo()
    let count = withUnsafeMutablePointer(to: &info) { proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, $0, Int32(MemoryLayout<proc_bsdinfo>.size)) }
    if count == 0 { if kill(pid, 0) != 0 && errno == ESRCH { return ["pid": pid, "state": "EXITED"] }; try fail("RESOURCE_MONITOR_IDENTITY_UNAVAILABLE") }
    guard Int(count) == MemoryLayout<proc_bsdinfo>.size else { try fail("RESOURCE_MONITOR_IDENTITY_UNAVAILABLE") }
    if info.pbi_uid != geteuid() { return ["pid": pid, "ownerUid": info.pbi_uid, "created": "\(info.pbi_start_tvsec):\(info.pbi_start_tvusec)"] }
    var buffer = [CChar](repeating: 0, count: Int(AGBRIO_RESOURCE_PATH_CAPACITY))
    let length = buffer.withUnsafeMutableBytes { proc_pidpath(pid, $0.baseAddress, UInt32($0.count)) }
    guard length > 0 else { try fail("RESOURCE_MONITOR_IDENTITY_UNAVAILABLE") }
    return ["pid": pid, "ownerUid": info.pbi_uid, "created": "\(info.pbi_start_tvsec):\(info.pbi_start_tvusec)", "image": URL(fileURLWithPath: String(cString: buffer)).resolvingSymlinksInPath().path]
}
func physicalAndAvailable() throws -> (UInt64, UInt64) {
    let task = Process(), pipe = Pipe()
    task.executableURL = URL(fileURLWithPath: "/usr/bin/memory_pressure"); task.arguments = ["-Q"]
    task.environment = ["PATH": "/usr/bin:/bin", "LC_ALL": "C"]; task.standardOutput = pipe; task.standardError = FileHandle.nullDevice
    try task.run()
    let output = try pipe.fileHandleForReading.read(upToCount: 32768) ?? Data(); task.waitUntilExit()
    let text = String(decoding: output, as: UTF8.self)
    guard task.terminationStatus == 0,
          let range = text.range(of: "System-wide memory free percentage: "),
          let end = text[range.upperBound...].firstIndex(of: "%"),
          let percent = Double(text[range.upperBound..<end]), percent.isFinite, (0...100).contains(percent) else { try fail("RESOURCE_MONITOR_MEMORY_UNAVAILABLE") }
    let physical = ProcessInfo.processInfo.physicalMemory
    guard physical > 0 else { try fail("RESOURCE_MONITOR_MEMORY_UNAVAILABLE") }
    return (physical, UInt64(Double(physical) * percent / 100))
}
func pressure() -> Int32? {
    var value: Int32 = 0, size = MemoryLayout<Int32>.size
    guard sysctlbyname("kern.memorystatus_vm_pressure_level", &value, &size, nil, 0) == 0,
          size == MemoryLayout<Int32>.size, [1, 2, 4].contains(value) else { return nil }
    return value
}
func volumes() -> [[String: Any]] {
    let keys: Set<URLResourceKey> = [.volumeIsLocalKey, .volumeNameKey, .volumeUUIDStringKey, .volumeTotalCapacityKey, .volumeAvailableCapacityForImportantUsageKey]
    let urls = [URL(fileURLWithPath: "/")] + (FileManager.default.mountedVolumeURLs(includingResourceValuesForKeys: Array(keys), options: [.skipHiddenVolumes]) ?? [])
    var rows = [[String: Any]](), seen = Set<String>()
    for url in urls {
        guard url.path == "/" || url.path.hasPrefix("/Volumes/"), let local = try? url.resourceValues(forKeys: [.volumeIsLocalKey]), local.volumeIsLocal == true,
              let value = try? url.resourceValues(forKeys: keys),
              let raw = value.volumeUUIDString, let uuid = UUID(uuidString: raw), let total = value.volumeTotalCapacity,
              let free = value.volumeAvailableCapacityForImportantUsage, total > 0, free >= 0, free <= Int64(total) else { continue }
        let identity = uuid.uuidString.lowercased(); if !seen.insert(identity).inserted { continue }
        rows.append(["drive": identity, "mountPoint": url.path, "label": value.volumeName ?? url.lastPathComponent,
                     "totalBytes": total, "freeBytes": free, "freePercent": Double(free) * 100 / Double(total)])
        if rows.count >= 64 { break }
    }
    return rows.sorted { ($0["mountPoint"] as! String) < ($1["mountPoint"] as! String) }
}
func topProcesses() -> [[String: Any]] {
    let estimate = proc_listallpids(nil, 0)
    guard estimate > 0 else { return [] }
    var pids = [Int32](repeating: 0, count: min(Int(estimate) + 64, 10000))
    let count = pids.withUnsafeMutableBytes { proc_listallpids($0.baseAddress, Int32($0.count)) }
    var rows = [[String: Any]]()
    for pid in pids.prefix(max(0, min(Int(count), pids.count))) where pid > 0 {
        var owner = proc_bsdinfo(), task = proc_taskinfo()
        let own = withUnsafeMutablePointer(to: &owner) { proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, $0, Int32(MemoryLayout<proc_bsdinfo>.size)) }
        guard Int(own) == MemoryLayout<proc_bsdinfo>.size else { continue }
        let read = withUnsafeMutablePointer(to: &task) { proc_pidinfo(pid, PROC_PIDTASKINFO, 0, $0, Int32(MemoryLayout<proc_taskinfo>.size)) }
        guard Int(read) == MemoryLayout<proc_taskinfo>.size else { continue }
        var name = [CChar](repeating: 0, count: 256)
        let length = name.withUnsafeMutableBytes { proc_name(pid, $0.baseAddress, UInt32($0.count)) }
        guard length > 0 else { continue }
        rows.append(["name": String(cString: name), "pid": pid, "metricKind": "RESIDENT_SET_BYTES", "residentBytes": task.pti_resident_size])
    }
    return Array(rows.sorted { ($0["residentBytes"] as! UInt64) > ($1["residentBytes"] as! UInt64) }.prefix(10))
}
func configuration(_ root: String, _ marker: [String: Any]?, _ drives: [[String: Any]]) throws -> ([String: Any], Bool) {
    let stored: [String: Any]? = marker == nil || (marker?["prepared"] as? Bool == true && !FileManager.default.fileExists(atPath: root + "/settings.json")) ? nil : try object(root + "/settings.json")
    let old = stored?["Disks"] as? [[String: Any]] ?? []
    let rules: [[String: Any]] = drives.map { drive in
        let identity = drive["drive"] as! String
        return old.first { $0["Drive"] as? String == identity } ?? ["Drive": identity, "Enabled": false, "FreePercent": 5.0]
    }
    var config = stored ?? ["SchemaVersion": 3, "Enabled": false, "SampleSeconds": 120, "AvailableMemoryPercent": 10.0,
                            "CommitPercent": 0.0, "RepeatMinutes": 30, "LegacyDiskFreeGiB": NSNull()]
    config["Disks"] = rules
    guard config["SchemaVersion"] as? Int == 3, config["CommitPercent"] as? Double == 0,
          let threshold = config["AvailableMemoryPercent"] as? Double, (1...95).contains(threshold),
          let interval = config["SampleSeconds"] as? Int, (60...3600).contains(interval),
          let repeatMinutes = config["RepeatMinutes"] as? Int, (5...1440).contains(repeatMinutes) else { try fail("RESOURCE_MONITOR_SETTINGS_INVALID") }
    return (config, Set(old.compactMap { $0["Drive"] as? String }) != Set(rules.compactMap { $0["Drive"] as? String }))
}
func atomic(_ value: Any, _ target: String) throws {
    let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    let temporary = (target as NSString).deletingLastPathComponent + "/.agbrio-" + UUID().uuidString + ".tmp"
    let fd = open(temporary, O_CREAT | O_EXCL | O_WRONLY | O_NOFOLLOW | O_CLOEXEC, 0o600)
    guard fd >= 0 else { try fail("RESOURCE_MONITOR_STORAGE_UNAVAILABLE") }
    defer { close(fd); unlink(temporary) }
    try data.withUnsafeBytes { buffer in
        var offset = 0
        while offset < buffer.count {
            let count = write(fd, buffer.baseAddress!.advanced(by: offset), buffer.count - offset)
            guard count > 0 else { try fail("RESOURCE_MONITOR_WRITE_UNCONFIRMED") }; offset += count
        }
    }
    guard fsync(fd) == 0, rename(temporary, target) == 0 else { try fail("RESOURCE_MONITOR_WRITE_UNCONFIRMED") }
}
func hashSelf() throws -> String {
    let file = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath()
    return SHA256.hash(data: try Data(contentsOf: file)).map { String(format: "%02x", $0) }.joined()
}
func sample(_ root: String, _ marker: [String: Any], _ config: [String: Any], _ drives: [[String: Any]], _ physical: UInt64, _ available: UInt64, _ level: Int32?, observed: Date) throws {
    guard config["Enabled"] as? Bool == true else { return }
    guard marker["binarySha256"] as? String == (try hashSelf()) else { try fail("RESOURCE_MONITOR_BINARY_CHANGED") }
    var conditions = [String](), metrics = [[String: Any]]()
    let memoryThreshold = config["AvailableMemoryPercent"] as! Double
    if Double(available) * 100 / Double(physical) < memoryThreshold { conditions.append("physical_memory_low") }
    if level == 2 || level == 4 { conditions.append("macos_memory_pressure") }
    for rule in config["Disks"] as! [[String: Any]] where rule["Enabled"] as? Bool == true {
        guard let identity = rule["Drive"] as? String, let drive = drives.first(where: { $0["drive"] as? String == identity }),
              let threshold = rule["FreePercent"] as? Double, (0.1...95).contains(threshold) else { try fail("RESOURCE_MONITOR_DRIVE_UNAVAILABLE") }
        metrics.append(["drive": identity, "label": drive["label"]!, "mountPoint": drive["mountPoint"]!, "totalBytes": drive["totalBytes"]!, "freeBytes": drive["freeBytes"]!,
                        "freePercent": drive["freePercent"]!, "thresholdPercent": threshold])
        if (drive["freePercent"] as! Double) < threshold { conditions.append("volume_low:" + identity) }
    }
    if conditions.isEmpty { return }
    conditions.sort()
    let now = observed, milliseconds = Int64(now.timeIntervalSince1970 * 1000)
    let severity = level == 4 || Double(available) * 100 / Double(physical) < 5 ? "critical" : "warning"
    if let state = try? object(root + "/state.json"), state["severity"] as? String == severity, state["conditions"] as? [String] == conditions,
       let stamp = state["observedAt"] as? Int64, milliseconds >= stamp, milliseconds - stamp < Int64((config["RepeatMinutes"] as! Int) * 60000) { return }
    let formatter = ISO8601DateFormatter(); formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let event: [String: Any] = ["schemaVersion": 3, "sourcePlatform": "MACOS", "eventId": UUID().uuidString.lowercased(),
        "eventType": "local_resource_pressure", "host": marker["hostId"]!, "observedUtc": formatter.string(from: now), "isTest": marker["isTest"] as? Bool ?? false,
        "severity": severity, "conditions": conditions, "metrics": ["kind": "MACOS_MEMORY_PRESSURE_QUERY", "physicalBytes": physical,
        "availableBytes": available, "availableThresholdPercent": memoryThreshold, "pressureLevel": level.map { $0 as Any } ?? NSNull(), "disks": metrics],
        "topProcesses": topProcesses(), "policy": ["observeOnly": true, "processActionsAllowed": false]]
    let outbox = root + "/outbox"
    if mkdir(outbox, 0o700) != 0 && errno != EEXIST { try fail("RESOURCE_MONITOR_STORAGE_UNAVAILABLE") }
    var folder = stat()
    guard lstat(outbox, &folder) == 0, (folder.st_mode & S_IFMT) == S_IFDIR, folder.st_uid == geteuid(), (folder.st_mode & 0o777) == 0o700 else { try fail("RESOURCE_MONITOR_STORAGE_UNTRUSTED") }
    let files = try FileManager.default.contentsOfDirectory(atPath: outbox)
    let target = files.count < 64 ? outbox + "/" + (event["eventId"] as! String) + ".json" : root + "/overflow-latest.json"
    try atomic(event, target)
    try atomic(["observedAt": milliseconds, "severity": severity, "conditions": conditions], root + "/state.json")
}
@main struct ResourceSampler {
    static func main() {
        do {
            guard CommandLine.arguments.count == 3 else { try fail("RESOURCE_MONITOR_COMMAND_INVALID") }
            if CommandLine.arguments[1] == "identity", let pid = Int32(CommandLine.arguments[2]) {
                print(String(data: try JSONSerialization.data(withJSONObject: processIdentity(pid), options: [.sortedKeys]), encoding: .utf8)!); return
            }
            guard ["snapshot", "sample"].contains(CommandLine.arguments[1]) else { try fail("RESOURCE_MONITOR_COMMAND_INVALID") }
            let root = CommandLine.arguments[2], marker = try trustedRoot(root)
            let observed = Date()
            let drives = volumes(), (physical, available) = try physicalAndAvailable(), level = pressure()
            let (config, changed) = try configuration(root, marker, drives)
            let storedConfig: [String: Any]? = marker == nil ? nil : (try? object(root + "/settings.json"))
            let storedRules = storedConfig?["Disks"] as? [[String: Any]] ?? []
            let unavailable = storedRules.contains { rule in rule["Enabled"] as? Bool == true && !drives.contains { $0["drive"] as? String == rule["Drive"] as? String } }
            if CommandLine.arguments[1] == "sample" {
                guard let marker else { try fail("RESOURCE_MONITOR_NOT_INSTALLED") }
                let fd = open(root + "/collector.lock", O_CREAT | O_RDWR | O_NOFOLLOW | O_CLOEXEC, 0o600)
                guard fd >= 0 else { try fail("RESOURCE_MONITOR_BUSY") }; defer { close(fd) }
                var lockInfo = stat()
                guard fstat(fd, &lockInfo) == 0, (lockInfo.st_mode & S_IFMT) == S_IFREG, lockInfo.st_uid == geteuid(), lockInfo.st_nlink == 1, (lockInfo.st_mode & 0o777) == 0o600 else { try fail("RESOURCE_MONITOR_STORAGE_UNTRUSTED") }
                guard flock(fd, LOCK_EX | LOCK_NB) == 0 else { try fail("RESOURCE_MONITOR_BUSY") }; defer { _ = flock(fd, LOCK_UN) }
                guard let current = try trustedRoot(root), current["prepared"] as? Bool != true else { try fail("RESOURCE_MONITOR_NOT_INSTALLED") }
                let (currentConfig, _) = try configuration(root, current, drives)
                guard !unavailable else { try fail("RESOURCE_MONITOR_DRIVE_UNAVAILABLE") }
                try sample(root, current, currentConfig, drives, physical, available, level, observed: observed)
            } else {
                let prepared = marker?["prepared"] as? Bool == true
                let value: [String: Any] = ["platform": "MACOS", "memoryMetric": "SYSTEM_FREE_PERCENT_ESTIMATE", "pressureLevel": level.map { $0 as Any } ?? NSNull(),
                    "config": config, "drives": drives, "physicalBytes": physical, "availableBytes": available, "installed": marker != nil && !prepared,
                    "running": false, "legacy": false, "startupEnabled": false, "selectionUnavailable": unavailable, "needsSave": changed || prepared, "processObservation": "ACCESSIBLE_RSS_PARTIAL"]
                print(String(data: try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys]), encoding: .utf8)!)
            }
        } catch Refusal.code(let code) { print("{\"errorCode\":\"\(code)\"}"); exit(1) }
        catch { print("{\"errorCode\":\"RESOURCE_MONITOR_UNAVAILABLE\"}"); exit(1) }
    }
}
