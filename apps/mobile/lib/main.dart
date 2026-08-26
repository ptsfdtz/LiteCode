import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';

void main() {
  runApp(const LiteCodeApp());
}

class LiteCodeApp extends StatelessWidget {
  const LiteCodeApp({super.key, this.autoConnect = true});

  final bool autoConnect;

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
      home: TaskHomePage(autoConnect: autoConnect),
    );
  }
}

enum AgentConnection { connecting, connected, disconnected }

class TaskHomePage extends StatefulWidget {
  const TaskHomePage({super.key, this.autoConnect = true});

  final bool autoConnect;

  @override
  State<TaskHomePage> createState() => _TaskHomePageState();
}

class _TaskHomePageState extends State<TaskHomePage> {
  static const _agentUri = 'ws://127.0.0.1:47831/v1/ws';

  final _promptController = TextEditingController(
    text: 'Create a file named litecode-e2e.txt containing exactly: '
        'Flutter -> Agent -> Codex works',
  );
  final _outputController = ScrollController();
  final List<String> _output = <String>[];
  WebSocket? _socket;
  StreamSubscription<dynamic>? _subscription;
  AgentConnection _connection = AgentConnection.disconnected;
  bool _running = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    if (widget.autoConnect) {
      unawaited(_connect());
    }
  }

  Future<void> _connect() async {
    await _subscription?.cancel();
    await _socket?.close();
    if (!mounted) return;
    setState(() {
      _connection = AgentConnection.connecting;
      _error = null;
    });
    try {
      final socket = await WebSocket.connect(_agentUri);
      if (!mounted) {
        await socket.close();
        return;
      }
      _socket = socket;
      _subscription = socket.listen(
        _handleMessage,
        onDone: _handleDisconnect,
        onError: (Object error) => _handleDisconnect(error.toString()),
        cancelOnError: true,
      );
      setState(() => _connection = AgentConnection.connected);
    } on Object {
      if (!mounted) return;
      setState(() {
        _connection = AgentConnection.disconnected;
        _error = 'Agent is not available on this computer.';
      });
    }
  }

  void _handleDisconnect([String? message]) {
    if (!mounted) return;
    setState(() {
      _connection = AgentConnection.disconnected;
      _running = false;
      _error = message ?? 'Agent connection closed.';
    });
  }

  void _handleMessage(dynamic data) {
    if (data is! String) return;
    final event = jsonDecode(data) as Map<String, dynamic>;
    final type = event['type'] as String? ?? 'unknown';
    switch (type) {
      case 'task_started':
        setState(() {
          _running = true;
          _error = null;
          _output.add('Task started');
        });
      case 'output_delta':
        final text = event['text'] as String? ?? '';
        setState(() => _output.add(_formatCodexEvent(text)));
      case 'task_completed':
        setState(() {
          _running = false;
          _output.add(event['summary'] as String? ?? 'Task completed');
        });
      case 'task_failed':
        setState(() {
          _running = false;
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
    if (_connection != AgentConnection.connected || prompt.isEmpty || _running) {
      return;
    }
    final taskId = 'task-${DateTime.now().microsecondsSinceEpoch}';
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
      _error = null;
      _output.clear();
      _output.add('Request sent to Codex');
    });
  }

  @override
  void dispose() {
    unawaited(_subscription?.cancel());
    unawaited(_socket?.close());
    _promptController.dispose();
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
                  Align(
                    alignment: Alignment.centerRight,
                    child: SizedBox(
                      width: 132,
                      height: 44,
                      child: FilledButton.icon(
                        onPressed: _connection == AgentConnection.connected && !_running
                            ? _runTask
                            : null,
                        icon: _running
                            ? const SizedBox.square(
                                dimension: 16,
                                child: CircularProgressIndicator(strokeWidth: 2),
                              )
                            : const Icon(Icons.play_arrow_rounded),
                        label: Text(_running ? 'Running' : 'Run task'),
                      ),
                    ),
                  ),
                  if (_error != null) ...[
                    const SizedBox(height: 12),
                    _ErrorBanner(message: _error!),
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
                              separatorBuilder: (_, _) => const SizedBox(height: 8),
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

class _ConnectionStatus extends StatelessWidget {
  const _ConnectionStatus({required this.connection, required this.onReconnect});

  final AgentConnection connection;
  final VoidCallback onReconnect;

  @override
  Widget build(BuildContext context) {
    final connected = connection == AgentConnection.connected;
    final label = switch (connection) {
      AgentConnection.connecting => 'Connecting',
      AgentConnection.connected => 'Agent online',
      AgentConnection.disconnected => 'Agent offline',
    };
    return TextButton.icon(
      onPressed: connection == AgentConnection.disconnected ? onReconnect : null,
      icon: Icon(
        connected ? Icons.check_circle : Icons.circle_outlined,
        size: 17,
        color: connected ? const Color(0xFF187A55) : null,
      ),
      label: Text(label),
    );
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
