import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';

const protocolVersion = 1;

class PairingInvitation {
  const PairingInvitation({
    required this.agentId,
    required this.endpoint,
    required this.secret,
    this.fingerprint,
  });

  final String agentId;
  final Uri endpoint;
  final String secret;
  final String? fingerprint;

  static PairingInvitation parse(String value) {
    final invitation = Uri.parse(value.trim());
    if (invitation.scheme != 'litecode' || invitation.host != 'pair') {
      throw const FormatException('This is not a LiteCode pairing invitation.');
    }
    final agentId = invitation.queryParameters['agent'];
    final encodedEndpoint = invitation.queryParameters['endpoint'];
    final secret = invitation.queryParameters['secret'];
    final fingerprint = invitation.queryParameters['fingerprint'];
    if (agentId == null || encodedEndpoint == null || secret == null) {
      throw const FormatException('The pairing invitation is incomplete.');
    }
    final endpoint = Uri.parse(
      utf8.decode(base64Url.decode(base64Url.normalize(encodedEndpoint))),
    );
    final loopback = endpoint.host == '127.0.0.1' ||
        endpoint.host == '::1' ||
        endpoint.host == 'localhost';
    if (endpoint.scheme == 'http' && !loopback) {
      throw const FormatException(
        'Network pairing requires HTTPS and a certificate fingerprint.',
      );
    } else if (endpoint.scheme == 'https' && fingerprint == null) {
      throw const FormatException('HTTPS pairing requires a certificate fingerprint.');
    } else if (endpoint.scheme != 'http' && endpoint.scheme != 'https') {
      throw const FormatException('The pairing endpoint scheme is not supported.');
    }
    if (fingerprint != null) {
      try {
        if (base64Url.decode(base64Url.normalize(fingerprint)).length != 32) {
          throw const FormatException('invalid fingerprint length');
        }
      } on FormatException {
        throw const FormatException('The certificate fingerprint is invalid.');
      }
    }
    return PairingInvitation(
      agentId: agentId,
      endpoint: endpoint,
      secret: secret,
      fingerprint: fingerprint,
    );
  }
}

class PairedAgent {
  const PairedAgent({
    required this.agentId,
    required this.deviceId,
    required this.endpoint,
    required this.credential,
    this.fingerprint,
  });

  final String agentId;
  final String deviceId;
  final Uri endpoint;
  final String credential;
  final String? fingerprint;

  Uri get websocketUri => endpoint.replace(
        scheme: endpoint.scheme == 'https' ? 'wss' : 'ws',
        path: '/v1/ws',
        query: null,
        fragment: null,
      );

  Map<String, dynamic> toJson() => <String, dynamic>{
        'agentId': agentId,
        'deviceId': deviceId,
        'endpoint': endpoint.toString(),
        'credential': credential,
        if (fingerprint != null) 'fingerprint': fingerprint,
      };

  factory PairedAgent.fromJson(Map<String, dynamic> json) => PairedAgent(
        agentId: json['agentId'] as String,
        deviceId: json['deviceId'] as String,
        endpoint: Uri.parse(json['endpoint'] as String),
        credential: json['credential'] as String,
        fingerprint: json['fingerprint'] as String?,
      );
}

abstract interface class AgentCredentialStore {
  Future<PairedAgent?> read();
  Future<void> write(PairedAgent agent);
  Future<void> clear();
}

class SecureAgentCredentialStore implements AgentCredentialStore {
  const SecureAgentCredentialStore();

  static const _storage = FlutterSecureStorage(
    aOptions: AndroidOptions(encryptedSharedPreferences: true),
  );
  static const _key = 'paired_agent_v1';

  @override
  Future<PairedAgent?> read() async {
    final value = await _storage.read(key: _key);
    if (value == null) return null;
    return PairedAgent.fromJson(jsonDecode(value) as Map<String, dynamic>);
  }

  @override
  Future<void> write(PairedAgent agent) =>
      _storage.write(key: _key, value: jsonEncode(agent.toJson()));

  @override
  Future<void> clear() => _storage.delete(key: _key);
}

class PairingClient {
  const PairingClient();

  Future<PairedAgent> pair(PairingInvitation invitation, String deviceName) async {
    final client = createPinnedHttpClient(invitation.fingerprint);
    try {
      final request = await client.postUrl(
        invitation.endpoint.replace(path: '/v1/pair', query: null, fragment: null),
      );
      request.headers.contentType = ContentType.json;
      request.write(jsonEncode(<String, dynamic>{
        'protocolVersion': protocolVersion,
        'pairingSecret': invitation.secret,
        'deviceName': deviceName,
      }));
      final response = await request.close();
      final body = await utf8.decoder.bind(response).join();
      if (response.statusCode != HttpStatus.ok) {
        throw PairingException(
          response.statusCode == HttpStatus.tooManyRequests
              ? 'Too many pairing attempts. Try again in one minute.'
              : 'The pairing invitation is invalid or has expired.',
        );
      }
      final json = jsonDecode(body) as Map<String, dynamic>;
      if (json['protocolVersion'] != protocolVersion ||
          json['agentId'] != invitation.agentId) {
        throw const PairingException('The Agent identity did not match the invitation.');
      }
      return PairedAgent(
        agentId: json['agentId'] as String,
        deviceId: json['deviceId'] as String,
        endpoint: invitation.endpoint,
        credential: json['deviceCredential'] as String,
        fingerprint: invitation.fingerprint,
      );
    } finally {
      client.close(force: true);
    }
  }
}

HttpClient createPinnedHttpClient(String? expectedFingerprint) {
  final client = HttpClient();
  if (expectedFingerprint != null) {
    client.badCertificateCallback = (certificate, _, _) {
      final actual = base64Url
          .encode(sha256.convert(certificate.der).bytes)
          .replaceAll('=', '');
      return actual == expectedFingerprint;
    };
  }
  return client;
}

class PairingException implements Exception {
  const PairingException(this.message);
  final String message;

  @override
  String toString() => message;
}
