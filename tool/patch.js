#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

const PATCH_FILE = process.argv[2];
if (!PATCH_FILE) {
    console.error('Uso: node patch.js <file_patch>');
    process.exit(1);
}

console.log(`📦 Applico patch da: ${PATCH_FILE}`);

const content = fs.readFileSync(PATCH_FILE, 'utf-8');
const lines = content.split('\n');

let currentFile = null;
let currentOperation = null;
let currentRange = null;
let patchLines = [];
let fileContent = [];
let operations = []; // accumula operazioni per applicarle in ordine

function flushFile() {
    if (!currentFile) return;
    
    // Scrivi il file se c'è contenuto
    if (fileContent.length > 0) {
        const dir = path.dirname(currentFile);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }
        fs.writeFileSync(currentFile, fileContent.join('\n'));
        console.log(`  💾 Salvato: ${currentFile}`);
    }
    
    currentFile = null;
    fileContent = [];
    operations = [];
}

// Applica tutte le operazioni in ordine
function applyOperations() {
    if (operations.length === 0) return;
    
    console.log(`\n  🔧 Applico ${operations.length} operazioni in ordine...`);
    
    for (const op of operations) {
        console.log(`    ${op.type} [${op.range}]`);
        
        switch (op.type) {
            case 'LINESUB': {
                const [start, end] = op.range.split(',').map(Number);
                const endLine = end || start;
                
                // Assicura che il file abbia abbastanza righe
                while (fileContent.length < endLine) {
                    fileContent.push('');
                }
                
                // Sostituisci righe
                for (let i = 0; i < op.lines.length; i++) {
                    fileContent[start - 1 + i] = op.lines[i];
                }
                // Se il range è più lungo delle patch, rimuovi righe extra
                if (op.lines.length < (endLine - start + 1)) {
                    fileContent.splice(start + op.lines.length - 1, (endLine - start + 1) - op.lines.length);
                }
                break;
            }
            
            case 'LINEADD': {
                const line = parseInt(op.range);
                // Assicura che il file abbia abbastanza righe
                while (fileContent.length < line - 1) {
                    fileContent.push('');
                }
                // Inserisci patchLines alla riga specificata
                fileContent.splice(line - 1, 0, ...op.lines);
                break;
            }
            
            case 'LINEDEL': {
                const [start, end] = op.range.split(',').map(Number);
                const endLine = end || start;
                
                if (fileContent.length >= start) {
                    fileContent.splice(start - 1, endLine - start + 1);
                }
                break;
            }
        }
    }
}

for (let line of lines) {
    line = line.trimRight();
    
    // Nuovo file
    const fileMatch = line.match(/^--\$FILE===:\s*(.+)$/);
    if (fileMatch) {
        flushFile();
        currentFile = fileMatch[1].trim();
        fileContent = [];
        operations = [];
        console.log(`\n📁 Nuovo file: ${currentFile}`);
        continue;
    }
    
    // Nuova operazione CHANGE
    const changeMatch = line.match(/^--\$CHANGE===:\s*(.+)$/);
    if (changeMatch) {
        flushFile();
        currentFile = changeMatch[1].trim();
        // Carica file esistente se presente
        if (fs.existsSync(currentFile)) {
            fileContent = fs.readFileSync(currentFile, 'utf-8').split('\n');
            console.log(`  📂 Caricato esistente (${fileContent.length} righe)`);
        } else {
            fileContent = [];
            console.log(`  📂 File nuovo, sarà creato`);
        }
        operations = [];
        console.log(`\n📁 Modifiche a: ${currentFile}`);
        continue;
    }
    
    // LineSub
    const subMatch = line.match(/^--\$LINESUB===:?(\d+(?:,\d+)?):?$/);
    if (subMatch) {
        currentOperation = 'LINESUB';
        currentRange = subMatch[1];
        patchLines = [];
        console.log(`    🔧 Accumulo sostituzione righe ${currentRange}`);
        continue;
    }
    
    // LineAdd
    const addMatch = line.match(/^--\$LINEADD===:?(\d+):?$/);
    if (addMatch) {
        if (currentOperation && patchLines.length > 0) {
            operations.push({
                type: currentOperation,
                range: currentRange,
                lines: [...patchLines]
            });
        }
        currentOperation = 'LINEADD';
        currentRange = addMatch[1];
        patchLines = [];
        console.log(`    ➕ Accumulo aggiunta alla riga ${currentRange}`);
        continue;
    }
    
    // LineDel
    const delMatch = line.match(/^--\$LINEDEL===:?(\d+(?:,\d+)?):?$/);
    if (delMatch) {
        if (currentOperation && patchLines.length > 0) {
            operations.push({
                type: currentOperation,
                range: currentRange,
                lines: [...patchLines]
            });
        }
        operations.push({
            type: 'LINEDEL',
            range: delMatch[1],
            lines: []
        });
        currentOperation = null;
        patchLines = [];
        console.log(`    ❌ Accumulo rimozione righe ${delMatch[1]}`);
        continue;
    }
    
    // Riga di contenuto per operazione corrente
    if (currentOperation) {
        patchLines.push(line);
    } else if (!currentOperation && currentFile) {
        // Contenuto del file (per --$FILE)
        fileContent.push(line);
    }
}

// Salva ultima operazione
if (currentOperation && patchLines.length > 0) {
    operations.push({
        type: currentOperation,
        range: currentRange,
        lines: [...patchLines]
    });
}

// Applica tutte le operazioni e salva
if (currentFile) {
    applyOperations();
    flushFile();
}

console.log('\n✅ Patch completata!');
