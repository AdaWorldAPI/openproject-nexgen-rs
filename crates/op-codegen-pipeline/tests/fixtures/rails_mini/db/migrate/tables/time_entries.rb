class Tables::TimeEntries < Tables::Base
  def self.table(migration)
    create_table migration do |t|
      t.float :hours
      t.references :work_package, null: false
      t.date :spent_on, null: false
    end
  end
end
