import 'package:flutter_test/flutter_test.dart';
import 'package:litecode_mobile/main.dart';
import 'package:litecode_mobile/pairing.dart';

class MemoryCredentialStore implements AgentCredentialStore {
  PairedAgent? agent;

  @override
  Future<void> clear() async => agent = null;

  @override
  Future<PairedAgent?> read() async => agent;

  @override
  Future<void> write(PairedAgent value) async => agent = value;
}

void main() {
  testWidgets('shows pairing when no device credential exists', (
    WidgetTester tester,
  ) async {
    await tester.pumpWidget(
      LiteCodeApp(
        autoConnect: false,
        credentialStore: MemoryCredentialStore(),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('LiteCode'), findsOneWidget);
    expect(find.text('Pair a computer'), findsOneWidget);
    expect(find.text('Pair device'), findsOneWidget);
  });

  testWidgets('shows tasks for a paired device', (WidgetTester tester) async {
    final store = MemoryCredentialStore()
      ..agent = PairedAgent(
        agentId: 'agent',
        deviceId: 'device',
        endpoint: Uri.parse('http://127.0.0.1:47831'),
        credential: 'credential',
      );
    await tester.pumpWidget(
      LiteCodeApp(autoConnect: false, credentialStore: store),
    );
    await tester.pumpAndSettle();

    expect(find.text('Task'), findsOneWidget);
    expect(find.text('Run task'), findsOneWidget);
    expect(find.text('No task activity'), findsOneWidget);
  });
}
