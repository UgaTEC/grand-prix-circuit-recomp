# Grand Prix Circuit (MS-DOS 1988) - Recompilación Estática Nativa en Rust

Recompilación nativa y portable en **Rust** del clásico simulador de Fórmula 1 **Grand Prix Circuit** (Accolade / Distinctive Software Inc., 1988), programado originalmente por Brad Gour, Don Mattrick y Amory Wong.

---

## 🚀 Características

- **Recompilación Estática Binaria**: Ejecuta la lógica original del binario `GPEGA.EXE` (desempaquetado desde su compresión original Microsoft EXEPACK) con fidelidad a las físicas y tiempos originales.
- **Renderizado Gráfico EGA Nativo**: Simulación por hardware del controlador gráfico EGA (modo `0x0D` de 320x200 y 16 colores, memoria plana de 4 planos y registros de retención/latches).
- **Ventana y Presentación Multiplataforma**: Integración con `minifb` escalada a 4x (1280x800) a 60 FPS fijos.
- **Capa HLE (High-Level Emulation) de MS-DOS y BIOS**:
  - `INT 21h`: Sistema de archivos case-insensitive mapeado automáticamente a la carpeta de assets `./gprix2`.
  - `INT 10h`: Modos de video y paleta EGA.
  - `INT 16h`: Cola de teclado no bloqueante.
  - `INT 1Ah` y PIT 8253: Temporización del sistema.
- **100% Portable**: No depende de emuladores externos pesados como DOSBox. Se compila de forma nativa en:
  - **Windows** (x86_64 / ARM64)
  - **Linux** (X11 / Wayland)
  - **macOS** (Metal / Cocoa)

---

## 📁 Estructura del Proyecto

```
gprix_recomp/
├── src/
│   ├── main.rs          # Bucle interactivo nativo, sincronización 60 FPS y teclado
│   ├── cpu.rs           # Contexto de CPU x86 de 16 bits (registros, banderas CF/ZF/SF/OF/DF/etc.)
│   ├── bus.rs           # Bus de 1 MB de memoria real, simulación EGA y renderizado a RGB
│   ├── loader.rs        # Cargador de binarios MZ con cálculo de PSP y relocalizaciones
│   ├── dos.rs           # Capa de servicios MS-DOS / BIOS HLE
│   └── recompiler.rs    # Decodificador y ejecutor de instrucciones 8086/80286 con iced-x86
├── Cargo.toml
└── README.md
```

---

## 🎮 Cómo Ejecutar

Para compilar y correr el juego nativo:

```bash
cd gprix_recomp
cargo run --release
```

### Controles
- **Espacio / Enter**: Seleccionar / Comenzar
- **Flechas Arriba / Abajo**: Acelerar / Frenar
- **Flechas Izquierda / Derecha**: Girar
- **1, 2, 3**: Selección de marchas
- **Escape**: Salir
