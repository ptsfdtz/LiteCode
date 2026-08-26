import 'package:flutter_test/flutter_test.dart';
import 'package:litecode_mobile/main.dart';

void main() {
  testWidgets('shows the empty device state', (WidgetTester tester) async {
    await tester.pumpWidget(const LiteCodeApp(autoConnect: false));

    expect(find.text('LiteCode'), findsOneWidget);
    expect(find.text('Task'), findsOneWidget);
    expect(find.text('Run task'), findsOneWidget);
    expect(find.text('No task activity'), findsOneWidget);
  });
}
