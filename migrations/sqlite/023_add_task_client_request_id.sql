-- Clé d'idempotence de la capture mobile hors-ligne.
--
-- La file de capture de la PWA rejoue un envoi dont la réponse s'est perdue.
-- Sans cette clé, un rejeu après un commit réussi côté serveur crée un
-- doublon -- dans une base qui en compte déjà beaucoup. La colonne est
-- NULLable et l'index est partiel : le chemin desktop n'envoie rien et
-- plusieurs tâches peuvent donc coexister avec la valeur NULL, ce qu'un
-- UNIQUE nu autoriserait aussi en SQLite mais que l'index partiel rend
-- explicite et moins cher.
ALTER TABLE tasks ADD COLUMN client_request_id TEXT;

CREATE UNIQUE INDEX idx_tasks_client_request_id
    ON tasks (user_id, client_request_id)
    WHERE client_request_id IS NOT NULL;
