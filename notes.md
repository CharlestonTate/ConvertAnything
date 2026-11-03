        TYPESCRIPT FRONTEND
                |
                v
          RUST / AXUM
                |
                v
       CONVERSION ENGINE
                |
       +--------+--------+
       |        |        |
     AUDIO   IMAGES   DOCUMENTS
       |        |        |
     FFMPEG  LIBVIPS  LIBREOFFICE
       |        |        |
       +--------+--------+
                |
                v
       RAYON / TOKIO
                |
                v
         OUTPUT MANAGER