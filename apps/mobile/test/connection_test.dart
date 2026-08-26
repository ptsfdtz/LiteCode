import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:litecode_mobile/main.dart';

void main() {
  test('reconnect delay uses bounded exponential backoff', () {
    expect(reconnectDelay(0), const Duration(seconds: 1));
    expect(reconnectDelay(2), const Duration(seconds: 4));
    expect(reconnectDelay(20), const Duration(seconds: 15));
  });

  test('connection failures provide actionable messages', () {
    expect(
      connectionErrorMessage(const SocketException('Connection refused')),
      contains('Check that it is running'),
    );
    expect(
      connectionErrorMessage(const WebSocketException('status code: 401')),
      contains('no longer authorized'),
    );
    expect(
      connectionErrorMessage(const HandshakeException('certificate mismatch')),
      contains('Pair it again'),
    );
  });

  test('event buffer restores order when live events arrive before replay', () {
    final buffer = OrderedEventBuffer();
    final tenth = <String, dynamic>{'sequence': 10};
    expect(buffer.add(10, tenth), isEmpty);

    final delivered = <Map<String, dynamic>>[];
    for (var sequence = 1; sequence <= 9; sequence++) {
      delivered.addAll(
        buffer.add(sequence, <String, dynamic>{'sequence': sequence}),
      );
    }

    expect(
      delivered.map((event) => event['sequence']),
      orderedEquals(<int>[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]),
    );
    expect(buffer.lastSequence, 10);
    expect(buffer.add(10, tenth), isEmpty);
  });
}
