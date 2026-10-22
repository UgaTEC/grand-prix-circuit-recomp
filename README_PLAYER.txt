================================================================================
           GRAND PRIX CIRCUIT (1988) - RECOMPILACION ESTATICA NATIVA
            Edicion Dual: MS-DOS + Commodore 64 con Suavizado 60 FPS
                       Accolade / Distinctive Software
================================================================================

Este paquete contiene la version nativa 64-bit de Grand Prix Circuit (1988),
recompilado estaticamente en Rust sin necesidad de DOSBox ni emuladores externos.

--------------------------------------------------------------------------------
REQUISITOS PREVIOS / ARCHIVOS DEL JUEGO:
--------------------------------------------------------------------------------
Para jugar, copia los archivos originales de tu copia legal de Grand Prix Circuit
en esta misma carpeta (o en una subcarpeta llamada 'gprix2/'):
  1. GPEGA.EXE (o GPEGA_UNP.EXE)
  2. Todos los archivos de pistas y coches (*.PES y *.PCS, ej. STATUS.PES,
     CAR1.PES, DETROIT.PES, MONACO.PES, etc.)
  3. (Opcional para audio C64): Grand_Prix_Circuit.sid

* Nota: El ejecutable cuenta con descompresor automatico de Microsoft EXEPACK,
  por lo que puedes utilizar directamente el GPEGA.EXE comercial original.

--------------------------------------------------------------------------------
TECLAS ESPECIALES Y MEJORAS MODERNAS:
--------------------------------------------------------------------------------
- F9:  ALTERNAR INTERPOLACION ESPACIAL A 60 FPS
       Por defecto, el juego cuenta con un interpolador espacial por flujo
       optico a 60 FPS reales que elimina los saltos del motor original de 20 FPS,
       haciendo que las curvas, el asfalto y los rivales se desplacen con suavidad
       geometrica continua, sin desenfoque de movimiento (motion blur) ni ghosting.
       El tablero, tacometro y volante permanecen 100% fijos y nitidos.
       Pulsa F9 para alternar entre:
         - [ 60 FPS INTERPOLATION ]: Maxima fluidez moderna a 60 FPS.
         - [ 20 FPS CLASSIC ]: Tasa de refresco original escalonada de 1988.

- F10: ALTERNAR MODO COMMODORE 64 (C64) / MODO MS-DOS
       Alterna en caliente entre:
         - MODO MS-DOS: Paleta EGA original + PC Speaker / AdLib.
         - MODO COMMODORE 64: Paleta analogica VIC-II + Musica del chip SID 6581
           compuesta por Kris Hatlelid.

--------------------------------------------------------------------------------
CONTROLES DEL JUEGO:
--------------------------------------------------------------------------------
Generales:
  - F9:                Alternar Modo 60 FPS Interpolacion / 20 FPS Clasico
  - F10:               Alternar Modo DOS / Modo Commodore 64

En los Menus:
  - Flechas:           Seleccionar opcion
  - Barra Espaciadora: Confirmar eleccion
  - Enter:             Confirmar eleccion
  - Escape:            Volver / Pausar

Durante la Conduccion:
  - Acelerar:          Flecha Arriba (o NumPad 8)
  - Frenar:            Flecha Abajo (o NumPad 2)
  - Girar Izquierda:   Flecha Izquierda (o NumPad 4)
  - Girar Derecha:     Flecha Derecha (o NumPad 6)
  - Subir de marcha:   Barra Espaciadora (o Shift)
  - Bajar de marcha:   Enter (o Control / Alt)
  - Pausa:             Escape

================================================================================
