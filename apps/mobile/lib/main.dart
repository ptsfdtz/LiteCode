import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import 'pairing.dart';

void main() {
  runApp(const LiteCodeApp());
}

class LiteCodeApp extends StatelessWidget {
  const LiteCodeApp({
    super.key,
    this.autoConnect = true,
    this.credentialStore = const SecureAgentCredentialStore(),
    this.pairingClient = const PairingClient(),
  });

  final bool autoConnect;
  final AgentCredentialStore credentialStore;
  final PairingClient pairingClient;

  @override
  Widget build(BuildContext context) {
    const ink = Color(0xFF15201E);
    const paper = Color(0xFFF6F8F7);
    return MaterialApp(
      title: 'LiteCode',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF176B5B),
          surface: paper,
        ),
        scaffoldBackgroundColor: paper,
        textTheme: ThemeData.light().textTheme.apply(
          bodyColor: ink,
          displayColor: ink,
        ),
        inputDecorationTheme: const InputDecorationTheme(
          filled: true,
          fillColor: Colors.white,
          border: OutlineInputBorder(
            borderRadius: BorderRadius.all(Radius.circular(6)),
          ),
        ),
        useMaterial3: true,
      ),
      home: _AppHome(
        autoConnect: autoConnect,
        credentialStore: credentialStore,
        pairingClient: pairingClient,
      ),
    );
  }
}

class _AppHome extends StatefulWidget {
  const _AppHome({
    required this.autoConnect,
    required this.credentialStore,
    required this.pairingClient,
  });

  final bool autoConnect;
  final AgentCredentialStore credentialStore;
  final PairingClient pairingClient;

  @override
  State<_AppHome> createState() => _AppHomeState();
}

class _AppHomeState extends State<_AppHome> {
  PairedAgent? _agent;
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    PairedAgent? agent;
    try {
      agent = await widget.credentialStore.read();
    } on Object {
      agent = null;
    }
    if (!mounted) return;
    setState(() {
      _agent = agent;
      _loading = false;
    });
  }

  Future<void> _paired(PairedAgent agent) async {
    await widget.credentialStore.write(agent);
    if (mounted) setState(() => _agent = agent);
  }

  Future<void> _forget() async {
    await widget.credentialStore.clear();
    if (mounted) setState(() => _agent = null);
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Scaffold(body: Center(child: CircularProgressIndicator()));
    }
    final agent = _agent;
    if (agent == null) {
      return PairDevicePage(client: widget.pairingClient, onPaired: _paired);
    }
    return TaskHomePage(
      agent: agent,
      autoConnect: widget.autoConnect,
      onForgetDevice: _forget,
    );
  }
}

class PairDevicePage extends StatefulWidget {
  const PairDevicePage({
    super.key,
    required this.client,
    required this.onPaired,
  });

  final PairingClient client;
  final Future<void> Function(PairedAgent) onPaired;

  @override
  State<PairDevicePage> createState() => _PairDevicePageState();
}

class _PairDevicePageState extends State<PairDevicePage> {
  final _invitationController = TextEditingController();
  bool _pairing = false;
  String? _error;

  bool get _canScan => !kIsWeb && (Platform.isAndroid || Platform.isIOS);

  Future<void> _pair(String value) async {
    if (_pairing || value.trim().isEmpty) return;
    setState(() {
      _pairing = true;
      _error = null;
    });
    try {
      final invitation = PairingInvitation.parse(value);
      final agent = await widget.client.pair(invitation, _deviceName());
      await widget.onPaired(agent);
    } on FormatException catch (error) {
      if (mounted) setState(() => _error = error.message);
    } on PairingException catch (error) {
      if (mounted) setState(() => _error = error.message);
    } on Object {
      if (mounted) setState(() => _error = 'Could not reach the Agent.');
    } finally {
      if (mounted) setState(() => _pairing = false);
    }
  }

  String _deviceName() {
    if (kIsWeb) return 'Web client';
    if (Platform.isIOS) return 'iPhone';
    if (Platform.isAndroid) return 'Android phone';
    return '${Platform.operatingSystem} client';
  }

  Future<void> _scan() async {
    final invitation = await Navigator.of(
      context,
    ).push<String>(MaterialPageRoute(builder: (_) => const _ScannerPage()));
    if (invitation != null) {
      _invitationController.text = invitation;
      await _pair(invitation);
    }
  }

  @override
  void dispose() {
    _invitationController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('LiteCode')),
      body: SafeArea(
        child: Center(
          child: SingleChildScrollView(
            padding: const EdgeInsets.all(24),
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 520),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  const Icon(
                    Icons.devices_rounded,
                    size: 42,
                    color: Color(0xFF176B5B),
                  ),
                  const SizedBox(height: 18),
                  Text(
                    'Pair a computer',
                    style: Theme.of(context).textTheme.headlineSmall,
                  ),
                  const SizedBox(height: 20),
                  if (_canScan) ...[
                    SizedBox(
                      height: 48,
                      child: FilledButton.icon(
                        onPressed: _pairing ? null : _scan,
                        icon: const Icon(Icons.qr_code_scanner_rounded),
                        label: const Text('Scan pairing code'),
                      ),
                    ),
                    const SizedBox(height: 20),
                    const Row(
                      children: [
                        Expanded(child: Divider()),
                        Padding(
                          padding: EdgeInsets.symmetric(horizontal: 12),
                          child: Text('or paste invitation'),
                        ),
                        Expanded(child: Divider()),
                      ],
                    ),
                    const SizedBox(height: 20),
                  ],
                  TextField(
                    controller: _invitationController,
                    enabled: !_pairing,
                    minLines: 2,
                    maxLines: 4,
                    autocorrect: false,
                    decoration: const InputDecoration(
                      labelText: 'Pairing invitation',
                    ),
                  ),
                  if (_error != null) ...[
                    const SizedBox(height: 12),
                    _ErrorBanner(message: _error!),
                  ],
                  const SizedBox(height: 16),
                  SizedBox(
                    height: 48,
                    child: FilledButton.icon(
                      onPressed: _pairing
                          ? null
                          : () => _pair(_invitationController.text),
                      icon: _pairing
                          ? const SizedBox.square(
                              dimension: 16,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Icon(Icons.link_rounded),
                      label: Text(_pairing ? 'Pairing' : 'Pair device'),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _ScannerPage extends StatefulWidget {
  const _ScannerPage();

  @override
  State<_ScannerPage> createState() => _ScannerPageState();
}

class _ScannerPageState extends State<_ScannerPage> {
  bool _found = false;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('Scan pairing code')),
      body: MobileScanner(
        onDetect: (capture) {
          if (_found) return;
          final value = capture.barcodes.firstOrNull?.rawValue;
          if (value == null || !value.startsWith('litecode://pair?')) return;
          _found = true;
          Navigator.of(context).pop(value);
        },
      ),
    );
  }
}

enum AgentConnection { connecting, reconnecting, connected, disconnected }

enum TaskOutcome { completed, cancelled }

class TaskHomePage extends StatefulWidget {
  const TaskHomePage({
    super.key,
    required this.agent,
    required this.onForgetDevice,
    this.autoConnect = true,
  });

  final PairedAgent agent;
  final VoidCallback onForgetDevice;
  final bool autoConnect;

  @override
  State<TaskHomePage> createState() => _TaskHomePageState();
}

class _TaskHomePageState extends State<TaskHomePage> {
  final _promptController = TextEditingController(
    text:
        'Create a file named litecode-e2e.txt containing exactly: '
        'Flutter -> Agent -> Codex works',
  );
  final _outputController = ScrollController();
  final _followUpController = TextEditingController();
  final List<String> _output = <String>[];
  WebSocket? _socket;
  StreamSubscription<dynamic>? _subscription;
  Timer? _reconnectTimer;
  AgentConnection _connection = AgentConnection.disconnected;
  int _reconnectAttempt = 0;
  bool _connecting = false;
  bool _running = false;
  bool _cancellationPending = false;
  TaskOutcome? _outcome;
  String? _error;
  String? _activeTaskId;
  final OrderedEventBuffer _eventBuffer = OrderedEventBuffer();

  @override
  void initState() {
    super.initState();
    if (widget.autoConnect) {
      unawaited(_connect());
    }
  }

  Future<void> _connect() async {
    if (_connecting) return;
    _connecting = true;
    _reconnectTimer?.cancel();
    await _subscription?.cancel();
    await _socket?.close();
    _subscription = null;
    _socket = null;
    if (!mounted) {
      _connecting = false;
      return;
    }
    setState(() {
      _connection = _reconnectAttempt == 0
          ? AgentConnection.connecting
          : AgentConnection.reconnecting;
      _error = null;
    });
    try {
      final socket = await WebSocket.connect(
        widget.agent.websocketUri.toString(),
        headers: <String, dynamic>{
          HttpHeaders.authorizationHeader: 'Bearer ${widget.agent.credential}',
        },
        customClient: createPinnedHttpClient(widget.agent.fingerprint),
      );
      if (!mounted) {
        await socket.close();
        return;
      }
      _socket = socket;
      _subscription = socket.listen(
        _handleMessage,
        onDone: _handleDisconnect,
        onError: _handleDisconnect,
        cancelOnError: true,
      );
      _reconnectAttempt = 0;
      _connecting = false;
      setState(() => _connection = AgentConnection.connected);
      final activeTaskId = _activeTaskId;
      if (activeTaskId != null) {
        socket.add(
          jsonEncode(<String, dynamic>{
            'type': 'resume_events',
            'task_id': activeTaskId,
            'after_sequence': _eventBuffer.lastSequence,
          }),
        );
        if (_cancellationPending) {
          socket.add(
            jsonEncode(<String, dynamic>{
              'type': 'stop_task',
              'task_id': activeTaskId,
            }),
          );
        }
      }
    } on Object catch (error) {
      if (!mounted) return;
      _connecting = false;
      setState(() {
        _connection = AgentConnection.disconnected;
        _error = connectionErrorMessage(error);
      });
      _scheduleReconnect();
    }
  }

  void _handleDisconnect([Object? error]) {
    if (!mounted) return;
    _subscription = null;
    _socket = null;
    setState(() {
      _connection = AgentConnection.disconnected;
      _error = error == null
          ? 'Agent connection closed. Reconnecting...'
          : connectionErrorMessage(error);
    });
    _scheduleReconnect();
  }

  void _scheduleReconnect() {
    if (!widget.autoConnect ||
        !mounted ||
        _connecting ||
        _reconnectTimer?.isActive == true) {
      return;
    }
    final delay = reconnectDelay(_reconnectAttempt);
    _reconnectAttempt++;
    _reconnectTimer = Timer(delay, _connect);
  }

  void _handleMessage(dynamic data) {
    if (data is! String) return;
    final event = jsonDecode(data) as Map<String, dynamic>;
    final taskId = event['task_id'] as String?;
    final sequence = event['sequence'] as int?;
    if (taskId == null || sequence == null) return;
    if (_activeTaskId != null && taskId != _activeTaskId) return;
    _activeTaskId ??= taskId;
    for (final next in _eventBuffer.add(sequence, event)) {
      _applyEvent(next);
    }
  }

  void _applyEvent(Map<String, dynamic> event) {
    final type = event['type'] as String? ?? 'unknown';
    switch (type) {
      case 'task_started':
        setState(() {
          _running = true;
          _outcome = null;
          _error = null;
          _output.add('Task started');
        });
      case 'output_delta':
        final text = event['text'] as String? ?? '';
        setState(() => _output.add(_formatCodexEvent(text)));
      case 'task_completed':
        setState(() {
          _running = false;
          _cancellationPending = false;
          _outcome = TaskOutcome.completed;
          _output.add(event['summary'] as String? ?? 'Task completed');
        });
      case 'task_stopped':
        setState(() {
          _running = false;
          _cancellationPending = false;
          _outcome = TaskOutcome.cancelled;
          _output.add('Task cancelled');
        });
      case 'task_failed':
        setState(() {
          _running = false;
          _cancellationPending = false;
          _error = event['message'] as String? ?? 'Task failed';
        });
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_outputController.hasClients) {
        _outputController.animateTo(
          _outputController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 180),
          curve: Curves.easeOutCubic,
        );
      }
    });
  }

  String _formatCodexEvent(String line) {
    try {
      final json = jsonDecode(line) as Map<String, dynamic>;
      final type = json['type'] as String? ?? 'event';
      final item = json['item'];
      if (item is Map<String, dynamic>) {
        final text = item['text'];
        if (text is String && text.isNotEmpty) return text;
        final command = item['command'];
        if (command is String && command.isNotEmpty) return command;
        final itemType = item['type'];
        if (itemType is String) return '$type · $itemType';
      }
      return type.replaceAll('.', ' · ');
    } on Object {
      return line;
    }
  }

  void _runTask() {
    final prompt = _promptController.text.trim();
    if (_connection != AgentConnection.connected ||
        prompt.isEmpty ||
        _running) {
      return;
    }
    final taskId = 'task-${DateTime.now().microsecondsSinceEpoch}';
    _activeTaskId = taskId;
    _eventBuffer.reset();
    _socket?.add(
      jsonEncode(<String, dynamic>{
        'type': 'create_task',
        'task_id': taskId,
        'workspace_id': 'local',
        'tool': 'codex',
        'prompt': prompt,
      }),
    );
    setState(() {
      _running = true;
      _cancellationPending = false;
      _outcome = null;
      _error = null;
      _output.clear();
      _output.add('Request sent to Codex');
    });
  }

  void _stopTask() {
    final taskId = _activeTaskId;
    if (!_running ||
        _cancellationPending ||
        _connection != AgentConnection.connected ||
        taskId == null) {
      return;
    }
    _socket?.add(
      jsonEncode(<String, dynamic>{'type': 'stop_task', 'task_id': taskId}),
    );
    setState(() => _cancellationPending = true);
  }

  void _sendFollowUp() {
    final taskId = _activeTaskId;
    final input = _followUpController.text.trim();
    if (!_running ||
        _cancellationPending ||
        _connection != AgentConnection.connected ||
        taskId == null ||
        input.isEmpty) {
      return;
    }
    _socket?.add(
      jsonEncode(<String, dynamic>{
        'type': 'send_input',
        'task_id': taskId,
        'input': input,
      }),
    );
    setState(() {
      _followUpController.clear();
      _output.add('Follow-up sent');
    });
  }

  @override
  void dispose() {
    _reconnectTimer?.cancel();
    unawaited(_subscription?.cancel());
    unawaited(_socket?.close());
    _promptController.dispose();
    _followUpController.dispose();
    _outputController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('LiteCode'),
        actions: [
          Padding(
            padding: const EdgeInsets.only(right: 20),
            child: _ConnectionStatus(
              connection: _connection,
              onReconnect: _connect,
            ),
          ),
          PopupMenuButton<String>(
            tooltip: 'Device options',
            icon: const Icon(Icons.more_vert),
            onSelected: (value) {
              if (value == 'forget') widget.onForgetDevice();
            },
            itemBuilder: (_) => const [
              PopupMenuItem(
                value: 'forget',
                child: Text('Forget this computer'),
              ),
            ],
          ),
          const SizedBox(width: 8),
        ],
      ),
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 920),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  TextField(
                    controller: _promptController,
                    minLines: 3,
                    maxLines: 6,
                    enabled: !_running,
                    decoration: const InputDecoration(
                      labelText: 'Task',
                      alignLabelWithHint: true,
                    ),
                  ),
                  const SizedBox(height: 12),
                  TaskActionControl(
                    connected: _connection == AgentConnection.connected,
                    running: _running,
                    cancellationPending: _cancellationPending,
                    onRun: _runTask,
                    onStop: _stopTask,
                  ),
                  if (_running) ...[
                    const SizedBox(height: 12),
                    FollowUpControl(
                      controller: _followUpController,
                      enabled:
                          _connection == AgentConnection.connected &&
                          !_cancellationPending,
                      onChanged: (_) => setState(() {}),
                      onSend: _sendFollowUp,
                    ),
                  ],
                  if (_error != null) ...[
                    const SizedBox(height: 12),
                    _ErrorBanner(message: _error!),
                  ],
                  if (_outcome != null) ...[
                    const SizedBox(height: 12),
                    TaskOutcomeBanner(outcome: _outcome!),
                  ],
                  const SizedBox(height: 20),
                  const Text(
                    'Activity',
                    style: TextStyle(fontSize: 15, fontWeight: FontWeight.w700),
                  ),
                  const SizedBox(height: 8),
                  Expanded(
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        color: const Color(0xFF101715),
                        borderRadius: BorderRadius.circular(6),
                      ),
                      child: _output.isEmpty
                          ? const Center(
                              child: Text(
                                'No task activity',
                                style: TextStyle(color: Color(0xFF8FA09B)),
                              ),
                            )
                          : ListView.separated(
                              controller: _outputController,
                              padding: const EdgeInsets.all(16),
                              itemCount: _output.length,
                              separatorBuilder: (_, _) =>
                                  const SizedBox(height: 8),
                              itemBuilder: (context, index) => SelectableText(
                                _output[index],
                                style: const TextStyle(
                                  color: Color(0xFFD8E1DE),
                                  fontFamily: 'monospace',
                                  fontSize: 13,
                                  height: 1.45,
                                ),
                              ),
                            ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class TaskActionControl extends StatelessWidget {
  const TaskActionControl({
    super.key,
    required this.connected,
    required this.running,
    required this.cancellationPending,
    required this.onRun,
    required this.onStop,
  });

  final bool connected;
  final bool running;
  final bool cancellationPending;
  final VoidCallback onRun;
  final VoidCallback onStop;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.centerRight,
      child: SizedBox(
        width: 132,
        height: 44,
        child: FilledButton.icon(
          onPressed: connected && !cancellationPending
              ? (running ? onStop : onRun)
              : null,
          icon: cancellationPending
              ? const SizedBox.square(
                  dimension: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : Icon(running ? Icons.stop_rounded : Icons.play_arrow_rounded),
          label: Text(
            cancellationPending ? 'Stopping' : (running ? 'Stop' : 'Run task'),
          ),
        ),
      ),
    );
  }
}

class FollowUpControl extends StatelessWidget {
  const FollowUpControl({
    super.key,
    required this.controller,
    required this.enabled,
    required this.onChanged,
    required this.onSend,
  });

  final TextEditingController controller;
  final bool enabled;
  final ValueChanged<String> onChanged;
  final VoidCallback onSend;

  @override
  Widget build(BuildContext context) {
    final canSend = enabled && controller.text.trim().isNotEmpty;
    return TextField(
      controller: controller,
      enabled: enabled,
      minLines: 1,
      maxLines: 3,
      textInputAction: TextInputAction.send,
      onChanged: onChanged,
      onSubmitted: canSend ? (_) => onSend() : null,
      decoration: InputDecoration(
        labelText: 'Follow-up',
        suffixIcon: IconButton(
          tooltip: 'Send follow-up',
          onPressed: canSend ? onSend : null,
          icon: const Icon(Icons.send_rounded),
        ),
      ),
    );
  }
}

class TaskOutcomeBanner extends StatelessWidget {
  const TaskOutcomeBanner({super.key, required this.outcome});

  final TaskOutcome outcome;

  @override
  Widget build(BuildContext context) {
    final cancelled = outcome == TaskOutcome.cancelled;
    return Semantics(
      liveRegion: true,
      child: Row(
        children: [
          Icon(
            cancelled ? Icons.cancel_outlined : Icons.check_circle_outline,
            size: 20,
          ),
          const SizedBox(width: 8),
          Text(cancelled ? 'Task cancelled' : 'Task completed'),
        ],
      ),
    );
  }
}

class _ConnectionStatus extends StatelessWidget {
  const _ConnectionStatus({
    required this.connection,
    required this.onReconnect,
  });

  final AgentConnection connection;
  final VoidCallback onReconnect;

  @override
  Widget build(BuildContext context) {
    final connected = connection == AgentConnection.connected;
    final label = switch (connection) {
      AgentConnection.connecting => 'Connecting',
      AgentConnection.reconnecting => 'Reconnecting',
      AgentConnection.connected => 'Agent online',
      AgentConnection.disconnected => 'Agent offline',
    };
    return TextButton.icon(
      onPressed: connection == AgentConnection.disconnected
          ? onReconnect
          : null,
      icon: Icon(
        connected ? Icons.check_circle : Icons.circle_outlined,
        size: 17,
        color: connected ? const Color(0xFF187A55) : null,
      ),
      label: Text(label),
    );
  }
}

Duration reconnectDelay(int attempt) {
  const delays = <Duration>[
    Duration(seconds: 1),
    Duration(seconds: 2),
    Duration(seconds: 4),
    Duration(seconds: 8),
    Duration(seconds: 15),
  ];
  return delays[attempt.clamp(0, delays.length - 1)];
}

String connectionErrorMessage(Object error) {
  final message = error.toString().toLowerCase();
  if (message.contains('401') || message.contains('403')) {
    return 'This device is no longer authorized. Pair it again.';
  }
  if (message.contains('429')) {
    return 'Too many connection attempts. Retrying shortly.';
  }
  if (error is HandshakeException ||
      message.contains('certificate') ||
      message.contains('handshake')) {
    return 'The Agent certificate could not be verified. Pair it again.';
  }
  if (error is SocketException ||
      message.contains('connection refused') ||
      message.contains('failed host lookup')) {
    return 'The Agent is unreachable. Check that it is running and using the paired address.';
  }
  return 'The Agent connection failed. Retrying automatically.';
}

class OrderedEventBuffer {
  int lastSequence = 0;
  final Map<int, Map<String, dynamic>> _pending = <int, Map<String, dynamic>>{};

  List<Map<String, dynamic>> add(int sequence, Map<String, dynamic> event) {
    if (sequence <= lastSequence) return const <Map<String, dynamic>>[];
    _pending[sequence] = event;
    final ready = <Map<String, dynamic>>[];
    while (true) {
      final next = _pending.remove(lastSequence + 1);
      if (next == null) break;
      lastSequence++;
      ready.add(next);
    }
    return ready;
  }

  void reset() {
    lastSequence = 0;
    _pending.clear();
  }
}

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: BoxDecoration(
        color: const Color(0xFFFFEDE9),
        border: Border.all(color: const Color(0xFFEAB4A8)),
        borderRadius: BorderRadius.circular(6),
      ),
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        child: Row(
          children: [
            const Icon(Icons.error_outline, size: 18, color: Color(0xFF9C392C)),
            const SizedBox(width: 8),
            Expanded(child: Text(message)),
          ],
        ),
      ),
    );
  }
}
