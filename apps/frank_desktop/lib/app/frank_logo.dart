import 'package:flutter/material.dart';

import 'theme.dart';

/// The shared Frank mark used by the shell and the authentication surface.
class FrankLogo extends StatelessWidget {
  const FrankLogo({
    required this.size,
    this.semanticLabel = 'Frank',
    super.key,
  });

  final double size;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(size * 0.22),
      child: Image.asset(
        'assets/branding/frank-logo.png',
        width: size,
        height: size,
        fit: BoxFit.cover,
        semanticLabel: semanticLabel,
        errorBuilder: (context, error, stackTrace) {
          return Container(
            width: size,
            height: size,
            alignment: Alignment.center,
            color: FrankColors.aubergine.withValues(alpha: 0.14),
            child: const Text(
              'F',
              style: TextStyle(color: FrankColors.aubergineAccent),
            ),
          );
        },
      ),
    );
  }
}
