import 'package:flutter_test/flutter_test.dart';
import 'package:litecode_mobile/main.dart';

void main() {
  testWidgets('shows the empty device state', (WidgetTester tester) async {
    await tester.pumpWidget(const LiteCodeApp());

    expect(find.text('LiteCode'), findsOneWidget);
    expect(find.text('No computers connected'), findsOneWidget);
    expect(find.text('Pair computer'), findsOneWidget);
  });
}

