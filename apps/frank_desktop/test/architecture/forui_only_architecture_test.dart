import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('Frank-owned Flutter surfaces stay ForUI-only', () {
    // Keep the forbidden names split in the source so this architecture test
    // does not report itself as a violation. The Icons expression deliberately
    // requires a non-identifier before it, which allows FrankIcons.*.
    final forbidden = <({String name, RegExp pattern})>[
      (
        name:
            'package:flutter/'
            'material.dart',
        pattern: RegExp(r'package:flutter/material\.dart'),
      ),
      (
        name:
            'Material'
            'App',
        pattern: RegExp(
          r'\bMaterial'
          r'App\b',
        ),
      ),
      (
        name:
            'Icons'
            '.',
        pattern: RegExp(r'(?<![A-Za-z0-9_])Icons\.'),
      ),
      (
        name:
            'FrankDesktop'
            'Menu',
        pattern: RegExp(
          r'\bFrankDesktop'
          r'Menu\w*\b',
        ),
      ),
      (
        name:
            'FrankDesktop'
            'Select',
        pattern: RegExp(
          r'\bFrankDesktop'
          r'Select\w*\b',
        ),
      ),
      (
        name:
            'Frank'
            'MenuItem',
        pattern: RegExp(
          r'\bFrank'
          r'MenuItem\b',
        ),
      ),
      (
        name:
            'Frank'
            'MenuGroup',
        pattern: RegExp(
          r'\bFrank'
          r'MenuGroup\b',
        ),
      ),
      (
        name:
            'uses-material'
            '-design: true',
        pattern: RegExp(r'uses-material-design:\s*true'),
      ),
    ];
    final roots = [
      Directory('lib'),
      Directory('test'),
      Directory('integration_test'),
    ];
    final violations = <String>[];

    for (final root in roots) {
      if (!root.existsSync()) continue;
      for (final entity in root.listSync(recursive: true)) {
        if (entity is! File || !entity.path.endsWith('.dart')) continue;
        final source = entity.readAsStringSync();
        for (final forbiddenPattern in forbidden) {
          if (forbiddenPattern.pattern.hasMatch(source)) {
            violations.add('${entity.path}: ${forbiddenPattern.name}');
          }
        }
      }
    }
    final pubspec = File('pubspec.yaml');
    if (pubspec.existsSync()) {
      final source = pubspec.readAsStringSync();
      final pattern = forbidden.last;
      if (pattern.pattern.hasMatch(source)) {
        violations.add('pubspec.yaml: ${pattern.name}');
      }
    }

    expect(violations, isEmpty, reason: violations.join('\n'));
  });
}
