import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
#if canImport(Security)
import Security
#endif

/// Validates the Cloudflare Access JWT (Cf-Access-Jwt-Assertion) on every HTTP request:
/// RS256 signature against the team's public keys, issuer, audience tag, expiry and the one allowed email.
final class AccessVerifier {
    struct Failure: Error { var reason: String }
    struct Key { var n: Data; var e: Data }

    private let teamDomain: String
    private let audience: String
    private let allowedEmail: String
    private let fetchKeys: (String) -> [String: Key]?
    private let checkSignature: (Key, Data, Data) -> Bool
    private let lock = NSLock()
    private var keys: [String: Key] = [:]
    private var lastFetch = Date.distantPast

    init(teamDomain: String, audience: String, allowedEmail: String,
         fetchKeys: @escaping (String) -> [String: Key]? = AccessVerifier.fetchJWKS,
         checkSignature: @escaping (Key, Data, Data) -> Bool = AccessVerifier.rsaSHA256Verify) {
        self.teamDomain = teamDomain
        self.audience = audience.trimmingCharacters(in: .whitespaces)
        self.allowedEmail = allowedEmail.trimmingCharacters(in: .whitespaces).lowercased()
        self.fetchKeys = fetchKeys
        self.checkSignature = checkSignature
    }

    /// Returns the authenticated email.
    func verify(_ token: String, now: Date = Date()) -> Result<String, Failure> {
        let parts = token.split(separator: ".", omittingEmptySubsequences: false).map(String.init)
        guard parts.count == 3,
              let headerData = AccessVerifier.base64URL(parts[0]),
              let payloadData = AccessVerifier.base64URL(parts[1]),
              let signature = AccessVerifier.base64URL(parts[2]),
              let header = (try? JSONSerialization.jsonObject(with: headerData)) as? [String: Any],
              let claims = (try? JSONSerialization.jsonObject(with: payloadData)) as? [String: Any]
        else { return .failure(Failure(reason: "malformed token")) }

        guard header["alg"] as? String == "RS256", let kid = header["kid"] as? String else {
            return .failure(Failure(reason: "unsupported algorithm"))
        }
        guard let key = key(for: kid, now: now) else { return .failure(Failure(reason: "unknown signing key")) }
        guard checkSignature(key, Data("\(parts[0]).\(parts[1])".utf8), signature) else {
            return .failure(Failure(reason: "bad signature"))
        }

        guard claims["iss"] as? String == "https://\(teamDomain)" else { return .failure(Failure(reason: "wrong issuer")) }
        let auds = (claims["aud"] as? [String]) ?? (claims["aud"] as? String).map { [$0] } ?? []
        guard !audience.isEmpty, auds.contains(audience) else { return .failure(Failure(reason: "wrong audience")) }
        guard let exp = (claims["exp"] as? NSNumber)?.doubleValue, exp > now.timeIntervalSince1970 else {
            return .failure(Failure(reason: "expired"))
        }
        if let nbf = (claims["nbf"] as? NSNumber)?.doubleValue, nbf > now.timeIntervalSince1970 + 60 {
            return .failure(Failure(reason: "not yet valid"))
        }
        guard let email = (claims["email"] as? String)?.lowercased(), !allowedEmail.isEmpty, email == allowedEmail else {
            return .failure(Failure(reason: "identity not allowed"))
        }
        return .success(email)
    }

    private func key(for kid: String, now: Date) -> Key? {
        lock.lock(); defer { lock.unlock() }
        if let k = keys[kid] { return k }
        // Unknown key id: keys rotate, so refetch, at most once a minute.
        guard now.timeIntervalSince(lastFetch) > 60 else { return nil }
        lastFetch = now
        if let fresh = fetchKeys(teamDomain) { keys = fresh }
        return keys[kid]
    }

    // MARK: Cloudflare public keys

    static func fetchJWKS(teamDomain: String) -> [String: Key]? {
        guard let url = URL(string: "https://\(teamDomain)/cdn-cgi/access/certs"),
              let data = HTTPClient.get(url)?.body,
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let list = json["keys"] as? [[String: Any]] else { return nil }
        var out: [String: Key] = [:]
        for k in list {
            guard k["kty"] as? String == "RSA", let kid = k["kid"] as? String,
                  let n = (k["n"] as? String).flatMap(base64URL), let e = (k["e"] as? String).flatMap(base64URL) else { continue }
            out[kid] = Key(n: n, e: e)
        }
        return out.isEmpty ? nil : out
    }

    static func base64URL(_ s: String) -> Data? {
        var b = s.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while b.count % 4 != 0 { b += "=" }
        return Data(base64Encoded: b)
    }

    // MARK: RSA signature

    static func rsaSHA256Verify(key: Key, message: Data, signature: Data) -> Bool {
        #if canImport(Security)
        let attributes: [CFString: Any] = [kSecAttrKeyType: kSecAttrKeyTypeRSA, kSecAttrKeyClass: kSecAttrKeyClassPublic]
        guard let secKey = SecKeyCreateWithData(pkcs1(key) as CFData, attributes as CFDictionary, nil) else { return false }
        return SecKeyVerifySignature(secKey, .rsaSignatureMessagePKCS1v15SHA256, message as CFData, signature as CFData, nil)
        #else
        return false          // no verifier on this platform: fail closed
        #endif
    }

    /// DER RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
    static func pkcs1(_ key: Key) -> Data {
        func length(_ n: Int) -> [UInt8] {
            if n < 0x80 { return [UInt8(n)] }
            var bytes: [UInt8] = []
            var v = n
            while v > 0 { bytes.insert(UInt8(v & 0xff), at: 0); v >>= 8 }
            return [0x80 | UInt8(bytes.count)] + bytes
        }
        func integer(_ d: Data) -> [UInt8] {
            var b = Array(d.drop { $0 == 0 })
            if b.isEmpty { b = [0] }
            if b[0] & 0x80 != 0 { b.insert(0, at: 0) }
            return [0x02] + length(b.count) + b
        }
        let body = integer(key.n) + integer(key.e)
        return Data([0x30] + length(body.count) + body)
    }
}

/// Small synchronous HTTP client for the key fetch and the doctor checks.
enum HTTPClient {
    struct Response { var status: Int; var body: Data }

    static func get(_ url: URL, timeout: TimeInterval = 10) -> Response? {
        var r = URLRequest(url: url, timeoutInterval: timeout)
        r.httpMethod = "GET"
        return send(r)
    }

    static func postJSON(_ url: URL, body: String, headers: [String: String] = [:], timeout: TimeInterval = 10) -> Response? {
        var r = URLRequest(url: url, timeoutInterval: timeout)
        r.httpMethod = "POST"
        r.httpBody = Data(body.utf8)
        r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        r.setValue("application/json, text/event-stream", forHTTPHeaderField: "Accept")
        for (name, value) in headers { r.setValue(value, forHTTPHeaderField: name) }
        return send(r)
    }

    private static func send(_ request: URLRequest) -> Response? {
        final class Box: @unchecked Sendable { var value: Response? }
        let box = Box()
        let done = DispatchSemaphore(value: 0)
        URLSession.shared.dataTask(with: request) { data, response, _ in
            if let http = response as? HTTPURLResponse { box.value = Response(status: http.statusCode, body: data ?? Data()) }
            done.signal()
        }.resume()
        done.wait()
        return box.value
    }
}
