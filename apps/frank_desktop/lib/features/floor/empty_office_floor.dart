import 'package:flame/game.dart';
import 'package:flutter/material.dart';

import '../../app/theme.dart';

class EmptyOfficeFloor extends StatefulWidget {
  const EmptyOfficeFloor({super.key});

  @override
  State<EmptyOfficeFloor> createState() => _EmptyOfficeFloorState();
}

class _EmptyOfficeFloorState extends State<EmptyOfficeFloor> {
  late final EmptyOfficeGame _game = EmptyOfficeGame();

  @override
  Widget build(BuildContext context) {
    return Semantics(
      container: true,
      label: 'Empty office floor. Floor simulation will arrive later.',
      child: ClipRect(
        child: Stack(
          fit: StackFit.expand,
          children: [
            const ColoredBox(color: FrankColors.canvas),
            GameWidget(game: _game),
            Center(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: FrankColors.panel.withValues(alpha: 0.86),
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: FrankColors.border),
                ),
                child: const Padding(
                  padding: EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                  child: Text(
                    'OFFICE FLOOR · EMPTY FOR NOW',
                    style: TextStyle(
                      color: FrankColors.muted,
                      fontFamily: FrankTypography.monoFontFamily,
                      fontSize: 10,
                      letterSpacing: 1.1,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class EmptyOfficeGame extends FlameGame {
  @override
  void onGameResize(Vector2 size) {
    final focalPoint = camera.viewfinder.position.clone();
    super.onGameResize(size);
    camera.viewfinder.position = focalPoint;
  }
}
