# FFF

A new file explorer for those that don't want to wait to find what they know to exist.

## Description

Find your Files Fast (FFF) is a file explorer software designed for low-latency file search and retrieval. It was designed and created out of frustration with the slow and tedious process of using the Windows File Explorer.  
  
The trade-off for high-speed file retrieval (<500ms) is that you, the user, must first manually select which directories (folders) you want to have fast access to. The software does an initial crawl of the files to learn where they are and add tags based on file name, type, and content.  
  
Directories indexed by FFF are "alive" in the sense that any new files added are automatically indexed, any files that are renamed update, and any files moved or deleted are given a winderful last few monents, then handled appropriately.  
  
The Llama3.2 model scans the first few hundred words in text-based file formats to add tags based on content, and the Moondream model scans any image formats for the same purpose. This is nice when you have photos with poor names, as a cat photo titled "img_0123.jpg" can be searched and found with the keyword "cat."  
  
**Note** that this software is 100% offline and local. The AI models and their inputs/outputs stay on your computer and never touch the internet. The same goes for the actual files and their content. No data collection here.

## Getting started

First, this software relies on Ollama and two models for full functionality. While Ollama is not required for the software to work (or work well), the included AI/ML models will add features to tag the files you index based on their content, aiding in a faster and easier search.
  
Install Ollama however you wish and ensure it's running with:

```.sh
ollama serve
```

Then pull the two models, Llama3.2 and Moondream:

```.sh
ollama pull llama3.2
ollama pull moondream
```

Then either download the bundled software or build it from source with:

```.sh
cargo tauri build
```

## Authors and acknowledgment
This project was completed as part of coursework for East Carolina University's Spring 2026 Software Engineering II course. The project team members are:
* Garrison Mullen
* Kamal Habal
* Will Harding
* Luis Ramirez
* Zaine Musa

## License
MIT license. Do whatever you want.