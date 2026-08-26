import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:litecode_mobile/pairing.dart';

void main() {
  test('parses the loopback invitation emitted by the Agent', () {
    final endpoint = base64Url.encode(utf8.encode('http://127.0.0.1:47831'));
    final invitation = PairingInvitation.parse(
      'litecode://pair?agent=agent-1&endpoint=$endpoint&secret=secret-1',
    );

    expect(invitation.agentId, 'agent-1');
    expect(invitation.endpoint, Uri.parse('http://127.0.0.1:47831'));
    expect(invitation.secret, 'secret-1');
  });

  test('rejects an unencrypted network invitation', () {
    final endpoint = base64Url.encode(utf8.encode('http://192.168.1.10:47831'));

    expect(
      () => PairingInvitation.parse(
        'litecode://pair?agent=agent-1&endpoint=$endpoint&secret=secret-1',
      ),
      throwsFormatException,
    );
  });

  test('accepts an encrypted network invitation with a SHA-256 fingerprint', () {
    final endpoint = base64Url.encode(utf8.encode('https://192.168.1.10:47831'));
    final fingerprint = base64Url.encode(List<int>.filled(32, 7)).replaceAll('=', '');

    final invitation = PairingInvitation.parse(
      'litecode://pair?agent=agent-1&endpoint=$endpoint&secret=secret-1'
      '&fingerprint=$fingerprint',
    );

    expect(invitation.endpoint.scheme, 'https');
    expect(invitation.fingerprint, fingerprint);
  });

  test('builds the authenticated websocket endpoint', () {
    final agent = PairedAgent(
      agentId: 'agent-1',
      deviceId: 'device-1',
      endpoint: Uri.parse('http://127.0.0.1:47831'),
      credential: 'credential',
    );

    expect(agent.websocketUri, Uri.parse('ws://127.0.0.1:47831/v1/ws'));
  });
}
