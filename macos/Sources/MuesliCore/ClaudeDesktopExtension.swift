import Foundation

public enum ClaudeDesktopExtension {
    public static let name = "muesli"

    public enum State: Equatable, Sendable {
        case claudeMissing
        case notInstalled
        case installed(version: String)
        case outdated(installed: String, bundled: String)
    }

    public static var claudeDirectory: URL { ClaudeDesktopConfig.defaultDirectory }

    public static func extensionsDirectory(in claude: URL) -> URL {
        claude.appendingPathComponent("Claude Extensions", isDirectory: true)
    }

    public static func state(bundledVersion: String = MuesliInfo.version, claude: URL = claudeDirectory) -> State {
        guard FileManager.default.fileExists(atPath: claude.path) else { return .claudeMissing }
        guard let installed = installedVersion(claude: claude) else { return .notInstalled }
        return isOlder(installed, than: bundledVersion) ? .outdated(installed: installed, bundled: bundledVersion) : .installed(version: installed)
    }

    public static func installedVersion(claude: URL = claudeDirectory) -> String? {
        let root = extensionsDirectory(in: claude)
        let folders = (try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)) ?? []
        for folder in folders {
            guard let data = try? Data(contentsOf: folder.appendingPathComponent("manifest.json")),
                  let manifest = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                  manifest["name"] as? String == name else { continue }
            return manifest["version"] as? String ?? "0"
        }
        return nil
    }

    public static func manifest(version: String = MuesliInfo.version, binary: String = "server/muesli-mcp", icon: String = "icon.png") -> [String: Any] {
        [
            "manifest_version": "0.3",
            "name": name,
            "display_name": "Muesli",
            "version": version,
            "description": "Read and manage your Muesli meeting transcripts: recording status, the list of meetings, full transcripts, and renaming or deleting them.",
            "long_description": "Muesli is an on-device meeting transcriber for the macOS menu bar. This extension lets Claude read its status and transcripts, rename meetings, set attendees, rename speakers and delete meetings (deleted files go to the macOS Trash). Claude cannot start or stop a recording. The Muesli app must be installed, with AI assistant access switched on in Settings.",
            "author": ["name": "Muesli", "url": "https://github.com/bdhoine/muesli"],
            "homepage": "https://github.com/bdhoine/muesli",
            "icon": icon,
            "server": [
                "type": "binary",
                "entry_point": binary,
                "mcp_config": ["command": "${__dirname}/\(binary)", "args": [String](), "env": [String: String]()],
            ],
            "tools": [
                ["name": "get_status", "description": "Whether Muesli is running, detecting a meeting or recording, and the latest finished transcript."],
                ["name": "list_transcripts", "description": "List meetings known to Muesli, newest first."],
                ["name": "get_transcript", "description": "Read one meeting transcript by id."],
                ["name": "rename_transcript", "description": "Change a finished meeting's title."],
                ["name": "set_attendees", "description": "Replace a finished meeting's attendee list."],
                ["name": "rename_speaker", "description": "Rename a speaker label, e.g. Others to a person's name."],
                ["name": "delete_transcript", "description": "Delete a finished meeting; its files go to the Trash."],
            ],
            "compatibility": ["platforms": ["darwin"]],
        ]
    }

    public static func manifestJSON(version: String = MuesliInfo.version) -> Data {
        (try? JSONSerialization.data(withJSONObject: manifest(version: version), options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes])) ?? Data()
    }

    static func isOlder(_ a: String, than b: String) -> Bool {
        let x = a.split(separator: ".").map { Int($0) ?? 0 }
        let y = b.split(separator: ".").map { Int($0) ?? 0 }
        for i in 0..<max(x.count, y.count) {
            let l = i < x.count ? x[i] : 0, r = i < y.count ? y[i] : 0
            if l != r { return l < r }
        }
        return false
    }
}
