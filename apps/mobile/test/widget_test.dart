import 'package:flutter/material.dart';
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
      LiteCodeApp(autoConnect: false, credentialStore: MemoryCredentialStore()),
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
    expect(find.text('Agent offline'), findsOneWidget);
    expect(
      tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
      isNull,
    );
  });

  testWidgets('task screen fits a narrow Windows-sized viewport', (
    WidgetTester tester,
  ) async {
    tester.view.physicalSize = const Size(390, 844);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
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

    expect(find.text('Agent offline'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Stop is shown only while running and follows connection state', (
    WidgetTester tester,
  ) async {
    var stopped = false;

    Future<void> pumpControl({
      required bool connected,
      required bool running,
      bool pending = false,
    }) {
      return tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: TaskActionControl(
              connected: connected,
              running: running,
              cancellationPending: pending,
              onRun: () {},
              onStop: () => stopped = true,
            ),
          ),
        ),
      );
    }

    await pumpControl(connected: true, running: false);
    expect(find.text('Run task'), findsOneWidget);
    expect(find.text('Stop'), findsNothing);

    await pumpControl(connected: true, running: true);
    expect(find.text('Stop'), findsOneWidget);
    await tester.tap(find.byType(FilledButton));
    expect(stopped, isTrue);

    await pumpControl(connected: false, running: true);
    expect(find.text('Stop'), findsOneWidget);
    expect(
      tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
      isNull,
    );

    await pumpControl(connected: true, running: true, pending: true);
    expect(find.text('Stopping'), findsOneWidget);
    expect(
      tester.widget<FilledButton>(find.byType(FilledButton)).onPressed,
      isNull,
    );
  });

  testWidgets('cancelled outcome is clearly displayed', (
    WidgetTester tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(body: TaskOutcomeBanner(outcome: TaskOutcome.cancelled)),
      ),
    );

    expect(find.text('Task cancelled'), findsOneWidget);
    expect(find.byIcon(Icons.cancel_outlined), findsOneWidget);
  });

  testWidgets('follow-up input requires connection and non-empty text', (
    WidgetTester tester,
  ) async {
    final controller = TextEditingController();
    addTearDown(controller.dispose);
    var sent = false;

    Future<void> pumpControl({required bool enabled}) {
      return tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FollowUpControl(
              controller: controller,
              enabled: enabled,
              onChanged: (_) {},
              onSend: () => sent = true,
            ),
          ),
        ),
      );
    }

    await pumpControl(enabled: true);
    expect(find.text('Follow-up'), findsOneWidget);
    expect(
      tester.widget<IconButton>(find.byType(IconButton)).onPressed,
      isNull,
    );

    controller.text = 'Focus on the failing test';
    await pumpControl(enabled: false);
    expect(tester.widget<TextField>(find.byType(TextField)).enabled, isFalse);
    expect(
      tester.widget<IconButton>(find.byType(IconButton)).onPressed,
      isNull,
    );

    await pumpControl(enabled: true);
    await tester.tap(find.byType(IconButton));
    expect(sent, isTrue);
  });

  testWidgets('follow-up control fits a narrow layout', (
    WidgetTester tester,
  ) async {
    tester.view.physicalSize = const Size(320, 240);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final controller = TextEditingController(text: 'Keep going');
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FollowUpControl(
            controller: controller,
            enabled: true,
            onChanged: (_) {},
            onSend: () {},
          ),
        ),
      ),
    );

    expect(find.byTooltip('Send follow-up'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
