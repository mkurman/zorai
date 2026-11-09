const fs = require('fs');
const path = require('path');

function isSkillDocument(fileName, relativeDir, insidePackage) {
    const name = fileName.toLowerCase();
    if (name === 'skill.md') return true;
    return name.endsWith('.md') && relativeDir !== '' && !insidePackage;
}

function skillName(relativePath) {
    const parts = relativePath.split('/');
    const name = parts[parts.length - 1];
    if (name.toLowerCase() === 'skill.md') return parts[parts.length - 2] || null;
    return name.replace(/\.md$/i, '');
}

function listSkillDocuments(skillsRoot) {
    const root = path.resolve(skillsRoot);
    const documents = [];
    const pending = [{ relativeDir: '', insidePackage: false }];
    while (pending.length > 0) {
        const { relativeDir, insidePackage: parentInsidePackage } = pending.pop();
        let entries;
        try {
            entries = fs.readdirSync(path.join(root, relativeDir), { withFileTypes: true });
        } catch {
            continue;
        }
        const insidePackage = parentInsidePackage
            || entries.some((entry) => entry.isFile() && entry.name.toLowerCase() === 'skill.md');
        for (const entry of entries) {
            if (entry.name.startsWith('.')) continue;
            const relativePath = relativeDir ? `${relativeDir}/${entry.name}` : entry.name;
            if (entry.isDirectory()) {
                pending.push({ relativeDir: relativePath, insidePackage });
            } else if (entry.isFile() && isSkillDocument(entry.name, relativeDir, insidePackage)) {
                const name = skillName(relativePath);
                if (name) documents.push({ path: relativePath, name });
            }
        }
    }
    return documents.sort((left, right) => left.name.localeCompare(right.name) || left.path.localeCompare(right.path));
}

module.exports = { listSkillDocuments };
