import Foundation
#if canImport(Glibc)
import Glibc
#elseif canImport(Darwin)
import Darwin
#endif

struct HTTPRequest {
    var method: String
    var path: String
    var headers: [String: String]      // lowercased names
    var body: Data
}

struct HTTPResponse {
    var status: Int
    var headers: [String: String] = [:]
    var body = Data()

    static func text(_ status: Int, _ message: String) -> HTTPResponse {
        HTTPResponse(status: status, headers: ["Content-Type": "text/plain; charset=utf-8"], body: Data((message + "\n").utf8))
    }
}

/// Deliberately small HTTP/1.1 server: loopback only, one request per connection.
/// cloudflared is the only intended client.
final class HTTPServer {
    struct StartError: Error { var message: String }

    static let maxHeaderBytes = 64 * 1024
    static let maxBodyBytes = 1024 * 1024

    private let port: UInt16
    private let handler: (HTTPRequest) -> HTTPResponse

    init(port: UInt16, handler: @escaping (HTTPRequest) -> HTTPResponse) {
        self.port = port
        self.handler = handler
    }

    func run() throws -> Never {
        signal(SIGPIPE, SIG_IGN)
        #if canImport(Glibc)
        let fd = socket(AF_INET, Int32(SOCK_STREAM.rawValue), 0)
        #else
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        #endif
        guard fd >= 0 else { throw StartError(message: "socket() failed") }
        var yes: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, socklen_t(MemoryLayout<Int32>.size))

        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = port.bigEndian
        addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))          // never reachable from another machine
        let bound = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) }
        }
        guard bound == 0 else { close(fd); throw StartError(message: "cannot bind 127.0.0.1:\(port) (already in use?)") }
        guard listen(fd, 16) == 0 else { close(fd); throw StartError(message: "listen() failed") }

        let queue = DispatchQueue(label: "muesli.http", attributes: .concurrent)
        while true {
            let client = accept(fd, nil, nil)
            if client < 0 { continue }
            queue.async { self.serve(client) }
        }
    }

    private func serve(_ fd: Int32) {
        defer { close(fd) }
        var timeout = timeval(tv_sec: 15, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
        setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))

        let reader = Reader(fd: fd)
        let response: HTTPResponse
        var rejected = false
        switch HTTPServer.parse(reader) {
        case .success(let request): response = handler(request)
        case .failure(let e): response = .text(e.status, e.message); rejected = true
        }
        var head = "HTTP/1.1 \(response.status) \(HTTPServer.reason(response.status))\r\n"
        var headers = response.headers
        headers["Content-Length"] = String(response.body.count)
        headers["Connection"] = "close"
        for (k, v) in headers.sorted(by: { $0.key < $1.key }) { head += "\(k): \(v)\r\n" }
        head += "\r\n"
        HTTPServer.writeAll(fd, Array(head.utf8) + Array(response.body))
        if rejected { HTTPServer.drainUnreadRequest(fd) }
    }

    static let maxDrainBytes = 4 * maxBodyBytes

    static func drainUnreadRequest(_ fd: Int32) {
        shutdown(fd, Int32(SHUT_WR))
        var timeout = timeval(tv_sec: 2, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
        var chunk = [UInt8](repeating: 0, count: 16 * 1024)
        var drained = 0
        while drained < maxDrainBytes {
            let n = recv(fd, &chunk, chunk.count, 0)
            if n <= 0 { break }
            drained += n
        }
    }

    // MARK: Parsing

    struct ParseError: Error { var status: Int; var message: String }

    /// Buffered byte source. `read` returns [] at end of stream.
    final class Reader {
        private let read: () -> [UInt8]
        private var buffer: [UInt8] = []
        private var position = 0

        init(read: @escaping () -> [UInt8]) { self.read = read }

        convenience init(fd: Int32) {
            self.init {
                var chunk = [UInt8](repeating: 0, count: 16 * 1024)
                let n = recv(fd, &chunk, chunk.count, 0)
                return n > 0 ? Array(chunk[..<n]) : []
            }
        }

        private func fill() -> Bool {
            let more = read()
            buffer.append(contentsOf: more)
            return !more.isEmpty
        }

        /// One line without its CRLF; nil at end of stream or when longer than `limit`.
        func line(limit: Int) -> String? {
            while true {
                if let nl = buffer[position...].firstIndex(of: 0x0A) {
                    var end = nl
                    if end > position && buffer[end - 1] == 0x0D { end -= 1 }
                    let s = String(decoding: buffer[position..<end], as: UTF8.self)
                    position = nl + 1
                    return s
                }
                if buffer.count - position > limit || !fill() { return nil }
            }
        }

        func bytes(_ n: Int) -> [UInt8]? {
            while buffer.count - position < n { if !fill() { return nil } }
            defer { position += n }
            return Array(buffer[position..<position + n])
        }
    }

    static func parse(_ reader: Reader) -> Result<HTTPRequest, ParseError> {
        guard let requestLine = reader.line(limit: maxHeaderBytes) else { return .failure(ParseError(status: 400, message: "Bad request")) }
        let parts = requestLine.split(separator: " ")
        guard parts.count == 3, parts[2].hasPrefix("HTTP/1.") else { return .failure(ParseError(status: 400, message: "Bad request")) }

        var headers: [String: String] = [:]
        var headerBytes = 0
        while true {
            guard let line = reader.line(limit: maxHeaderBytes) else { return .failure(ParseError(status: 400, message: "Bad request")) }
            if line.isEmpty { break }
            headerBytes += line.utf8.count
            guard headerBytes <= maxHeaderBytes, let colon = line.firstIndex(of: ":") else {
                return .failure(ParseError(status: 431, message: "Headers too large or malformed"))
            }
            headers[line[..<colon].lowercased()] = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        }

        var body: [UInt8] = []
        if headers["transfer-encoding"]?.lowercased().contains("chunked") == true {
            while true {
                guard let sizeLine = reader.line(limit: 1024),
                      let size = Int(sizeLine.split(separator: ";").first.map(String.init)?.trimmingCharacters(in: .whitespaces) ?? "", radix: 16),
                      size >= 0 else { return .failure(ParseError(status: 400, message: "Bad chunk")) }
                if size == 0 { break }
                guard body.count + size <= maxBodyBytes else { return .failure(ParseError(status: 413, message: "Body too large")) }
                guard let chunk = reader.bytes(size), reader.line(limit: 16) != nil else { return .failure(ParseError(status: 400, message: "Bad chunk")) }
                body.append(contentsOf: chunk)
            }
        } else if let lengthText = headers["content-length"] {
            guard let length = Int(lengthText), length >= 0 else { return .failure(ParseError(status: 400, message: "Bad Content-Length")) }
            guard length <= maxBodyBytes else { return .failure(ParseError(status: 413, message: "Body too large")) }
            guard let b = reader.bytes(length) else { return .failure(ParseError(status: 400, message: "Incomplete body")) }
            body = b
        }

        var path = String(parts[1])
        if let q = path.firstIndex(of: "?") { path = String(path[..<q]) }
        return .success(HTTPRequest(method: String(parts[0]), path: path, headers: headers, body: Data(body)))
    }

    private static func writeAll(_ fd: Int32, _ bytes: [UInt8]) {
        var sent = 0
        while sent < bytes.count {
            let n = bytes[sent...].withUnsafeBytes { send(fd, $0.baseAddress, $0.count, 0) }
            if n <= 0 { return }
            sent += n
        }
    }

    private static func reason(_ status: Int) -> String {
        switch status {
        case 200: "OK"
        case 202: "Accepted"
        case 400: "Bad Request"
        case 401: "Unauthorized"
        case 404: "Not Found"
        case 405: "Method Not Allowed"
        case 413: "Payload Too Large"
        case 431: "Request Header Fields Too Large"
        default: "Error"
        }
    }
}
